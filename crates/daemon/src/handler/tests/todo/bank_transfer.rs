//! Bank method is factual source evidence, not another obligation verdict.
use super::*;
use mxr_core::asks::{ActionKind, Actor, AskAnswer};
use mxr_todo::provenance::FieldSources;
use mxr_todo::timing::{lead_time, TimingInput};
use mxr_todo::TodoKind;

mod persistent;

const TEXT: &str = "Invoice amount due £125 by 18 November 2026 via bank transfer.";

fn isolated_utc(f: impl FnOnce()) {
    const CHILD: &str = "MXR_BANK_TIMING_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // TZ is process-wide; parallel daemon tests must never observe this fixture's clock zone.
        let thread = std::thread::current();
        let name = thread.name().expect("named native test");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("TZ", "UTC")
            .env(CHILD, "1")
            .env("RUST_TEST_THREADS", "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated UTC test {name}: {status}");
        return;
    }
    f();
}

fn utc_test(f: impl std::future::Future<Output = ()>) {
    isolated_utc(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f);
    });
}

async fn bill(fx: &Fixture, text: &str) -> MessageId {
    let arrived = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
    let id = put(
        fx,
        Mail::new(
            ("Accounts", "accounts@example.com"),
            "Account copy",
            text,
            arrived,
        ),
    )
    .await;
    scan(fx, std::slice::from_ref(&id), arrived).await;
    id
}

async fn stored(fx: &Fixture, id: &MessageId) -> mxr_store::TodoRecord {
    let rows = fx
        .state
        .store
        .list_runway_todos(Some(&fx.account), DateTime::UNIX_EPOCH)
        .await
        .unwrap();
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r.source_message_id.as_ref() == Some(id))
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows.into_iter().next().unwrap()
}

async fn bank_journey(fx: &Fixture) -> (MessageId, mxr_store::TodoRecord) {
    let arrived = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
    assert_eq!(fx.state.snooze_time_prefs().morning_hour, 9);
    finish_first_run(fx, arrived).await;
    let id = bill(fx, TEXT).await;
    let answer = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&id))
        .await
        .unwrap()
        .remove(&id)
        .unwrap();
    let row = stored(fx, &id).await;
    eprintln!("REVIEW9_STORED answer={answer:?} row={row:?}");
    let units: Vec<_> = answer
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.answer == AskAnswer::AsksYou)
        .collect();
    assert_eq!(units.len(), 1);
    let (index, unit) = units[0];
    let tags = unit.tags.as_ref().unwrap();
    assert_eq!(tags.actor, Actor::Owner);
    assert_eq!(tags.action, ActionKind::Pay);
    assert_eq!(answer.quote(unit), Some(TEXT));
    assert_eq!(row.reason, TEXT);
    assert_eq!(row.amount_minor, Some(12500));
    assert_eq!(row.currency.as_deref(), Some("GBP"));
    let fields = FieldSources::from_json(&row.field_sources);
    assert_eq!(
        fields.get("asks_provenance").unwrap().evidence.as_deref(),
        Some(mxr_todo::asks::items::unit_provenance(&answer, index, unit).as_str())
    );
    let due = Utc.with_ymd_and_hms(2026, 11, 18, 23, 59, 59).unwrap();
    assert_eq!(row.due_at, Some(due));
    let expected = lead_time(
        &TimingInput {
            kind: TodoKind::Bill,
            doc_type: Some("bill_bank"),
            arrived,
            due: Some(due),
            event_start: None,
        },
        &Utc,
        9,
    );
    assert_eq!(
        expected.act_by.unwrap().date_naive(),
        chrono::NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()
    );
    assert_eq!(
        expected.surface_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap())
    );
    let at = Utc.with_ymd_and_hms(2026, 11, 11, 12, 0, 0).unwrap();
    let runway = runway(fx, at).await;
    let now = super::super::super::now::now_at(&fx.state, Some(&fx.account), at, &Utc)
        .await
        .unwrap();
    eprintln!(
        "REVIEW9_DELIVERY runway={:?} now={:?} expected={expected:?}",
        runway.now, now.due_soon.todos
    );
    assert_eq!(
        fields.0["payment_method"].evidence.as_deref(),
        Some("bank transfer")
    );
    assert!(fields.0["payment_method"].checked);
    assert_eq!(row.doc_type.as_deref(), Some("bill_bank"));
    assert_eq!(row.act_by_at, expected.act_by);
    assert_eq!(row.surface_at, expected.surface_at);
    assert!(runway.now.iter().any(|t| t.id == row.id));
    assert!(now.due_soon.todos.iter().any(|t| t.todo.id == row.id));
    scan(fx, std::slice::from_ref(&id), arrived).await;
    assert_eq!(stored(fx, &id).await, row);
    (id, row)
}

#[test]
fn bank_transfer_bill_keeps_its_stored_lead_time() {
    utc_test(async {
        bank_journey(&Fixture::new().await).await;
    });
}

#[test]
fn already_settled_policy16_bill_is_repaired_by_the_normal_tick() {
    utc_test(async {
        let fx = Fixture::new().await;
        let (id, expected) = bank_journey(&fx).await;
        let before = fx
            .state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&id))
            .await
            .unwrap();
        seed_published_row(&fx, &expected).await;
        let old = stored(&fx, &id).await;
        assert_eq!(old.doc_type, None);
        assert_eq!(
            fx.state
                .store
                .get_todo_run(&fx.account)
                .await
                .unwrap()
                .unwrap()
                .rules_version,
            2
        );
        finish_first_run(&fx, expected.source_date.unwrap()).await;
        let actual = stored(&fx, &id).await;
        eprintln!("REVIEW9_RETAINED old={old:?} after_tick={actual:?}");
        assert_eq!(actual.doc_type.as_deref(), Some("bill_bank"));
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.act_by_at, expected.act_by_at);
        assert_eq!(actual.surface_at, expected.surface_at);
        assert_eq!(
            fx.state
                .store
                .asks_for_messages(&fx.account, std::slice::from_ref(&id))
                .await
                .unwrap(),
            before
        );
    });
}

async fn seed_published_row(fx: &Fixture, expected: &mxr_store::TodoRecord) {
    // Faithful published current-policy row/run: generic dates, no method provenance, completed v2.
    let input = TimingInput {
        kind: TodoKind::Bill,
        doc_type: None,
        arrived: expected.source_date.unwrap(),
        due: expected.due_at,
        event_start: None,
    };
    let old = lead_time(&input, &Utc, 9);
    let mut fields = FieldSources::from_json(&expected.field_sources);
    fields.0.remove("payment_method");
    fields.0.get_mut("act_by_at").unwrap().evidence = Some("the due date".into());
    sqlx::query("UPDATE todos SET doc_type=NULL, act_by_at=?, surface_at=?, field_sources=?, rules_version=2 WHERE id=?")
        .bind(old.act_by.map(|at| at.timestamp())).bind(old.surface_at.map(|at| at.timestamp()))
        .bind(fields.to_json()).bind(&expected.id).execute(fx.state.store.writer()).await.unwrap();
    sqlx::query("UPDATE todo_runs SET rules_version=2 WHERE account_id=?")
        .bind(fx.account.as_str())
        .execute(fx.state.store.writer())
        .await
        .unwrap();
}

#[test]
fn payment_method_is_scoped_to_its_authored_obligation() {
    utc_test(async {
        for (text, link) in [
            ("Invoice amount due £125 by 18 November 2026.", false),
            ("Invoice amount due £125 by 18 November 2026.\n\nPay at https://example.com/pay", true),
            ("Invoice amount due £125 by 18 November 2026.\n\n> Previous payment used bank transfer.", false),
            ("Invoice amount due £125 by 18 November 2026.\n\n-- \nBank transfer support", false),
            ("Invoice amount due £125 by 18 November 2026 via bank transfer at https://example.com/pay.", true),
        ] {
            let fx = Fixture::new().await;
            let id = bill(&fx, text).await;
            let row = stored(&fx, &id).await;
            assert_eq!(row.doc_type, None, "{text}");
            assert_eq!(row.action_url.is_some(), link, "{text}");
            assert_eq!(row.act_by_at, row.due_at);
            assert!(!FieldSources::from_json(&row.field_sources).0.contains_key("payment_method"));
        }
        let fx = Fixture::new().await;
        let a = "Account #A123 amount due £125 by 18 November 2026 via bank transfer.";
        let b = "Account #B456 amount due €40 by 20 November 2026 at https://example.com/pay";
        let id = bill(&fx, &format!("{a}\n\n{b}")).await;
        let rows = fx
            .state
            .store
            .list_runway_todos(Some(&fx.account), DateTime::UNIX_EPOCH)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        let bank = rows
            .iter()
            .find(|r| r.currency.as_deref() == Some("GBP"))
            .unwrap();
        let link = rows
            .iter()
            .find(|r| r.currency.as_deref() == Some("EUR"))
            .unwrap();
        assert_eq!(bank.doc_type.as_deref(), Some("bill_bank"));
        assert_eq!(bank.reason, a);
        assert!(bank.action_url.is_none());
        assert_eq!(link.doc_type, None);
        assert_eq!(link.reason, b);
        assert_eq!(link.action_url.as_deref(), Some("https://example.com/pay"));
        assert_ne!(bank.dedup_key, link.dedup_key);
        scan(&fx, &[id], bank.source_date.unwrap()).await;
        assert_eq!(
            fx.state
                .store
                .list_runway_todos(Some(&fx.account), DateTime::UNIX_EPOCH)
                .await
                .unwrap(),
            rows
        );
        for text in [
            "I can pay invoice #K918 £25 by 1 November 2026 via bank transfer.",
            "She will pay invoice #K918 £25 by 1 November 2026 via bank transfer.",
            "Invoice paid £125 on 18 November 2026 via bank transfer.",
        ] {
            let fx = Fixture::new().await;
            bill(&fx, text).await;
            assert!(
                fx.state
                    .store
                    .list_runway_todos(Some(&fx.account), DateTime::UNIX_EPOCH)
                    .await
                    .unwrap()
                    .is_empty(),
                "{text}"
            );
        }
    });
}

#[test]
fn an_actual_legacy_bank_row_keeps_its_identity_and_allowance() {
    utc_test(async {
        let fx = Fixture::new().await;
        let arrived = Utc.with_ymd_and_hms(2026, 10, 24, 12, 0, 0).unwrap();
        finish_first_run(&fx, arrived).await;
        let id = put(
            &fx,
            Mail::new(
                ("Accounts", "accounts@example.com"),
                "Account copy",
                TEXT,
                arrived,
            ),
        )
        .await;
        // Build the retained v1 row with its real detector and natural key, never as a second live judge.
        let detection = mxr_todo::detect::detect(
            &mxr_todo::detect::MessageInput {
                subject: "Account copy",
                body_text: TEXT,
                body_html: None,
                from_name: Some("Accounts"),
                from_email: "accounts@example.com",
                sent: arrived,
                list_mail: false,
            },
            &Utc,
        )
        .unwrap();
        assert_eq!(detection.doc_type.as_deref(), Some("bill_bank"));
        let object = detection.object_key.clone();
        let domain = detection.sender_domain.clone().unwrap();
        let row = fx
            .state
            .store
            .list_todo_scan_rows(std::slice::from_ref(&id))
            .await
            .unwrap()
            .remove(0);
        let cfg = pass::PassConfig {
            now: arrived,
            tz: Utc,
            morning_hour: 9,
            catchup_days: 14,
            catchup_max: 20,
        };
        let (mut legacy, _) = pass::stored_record(&cfg, &row, detection, false);
        legacy.rules_version = 1;
        legacy.dedup_key = format!("bill|{domain}|{object}|due:2026-11-18");
        assert!(!legacy.has_stored_asks());
        fx.state.store.insert_todo(&legacy).await.unwrap();
        finish_first_run(&fx, arrived).await;
        let actual = stored(&fx, &id).await;
        assert_eq!(actual.id, legacy.id);
        assert_eq!(actual.doc_type, legacy.doc_type);
        assert_eq!(actual.act_by_at, legacy.act_by_at);
        assert_eq!(actual.surface_at, legacy.surface_at);
        assert_eq!(actual.reason, TEXT);
        assert!(actual.has_stored_asks());
        // Legacy adoption records the canonical cutover, independent of the first-run replay version.
        assert_eq!(actual.rules_version, 2);
        finish_first_run(&fx, arrived).await;
        assert_eq!(stored(&fx, &id).await, actual);
    });
}

#[test]
fn replay_preserves_saved_fields_closed_decisions_and_exact_correction_undo() {
    utc_test(async {
        for decision in ["fields", "done", "dismissed"] {
            let fx = Fixture::new().await;
            let (id, current) = bank_journey(&fx).await;
            seed_published_row(&fx, &current).await;
            let original = stored(&fx, &id).await;
            if decision == "fields" {
                let ResponseData::TodoChange { change } = request(
                    &fx,
                    Request::CorrectTodo {
                        todo_id: current.id.clone(),
                        reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                        edits: [
                            ("title", "My payment"),
                            ("kind", "lease"),
                            ("amount", "£140"),
                            ("due", "2026-12-10"),
                        ]
                        .into_iter()
                        .map(|(field, value)| mxr_protocol::TodoEditData {
                            field: field.into(),
                            value: value.into(),
                        })
                        .collect(),
                        time_zone: Some("UTC".into()),
                        dry_run: false,
                    },
                )
                .await
                else {
                    panic!("correction")
                };
                assert!(change.correction_undo.is_some());
            } else {
                request(
                    &fx,
                    Request::SetTodoState {
                        todo_ids: vec![current.id.clone()],
                        action: if decision == "done" {
                            TodoStateActionData::Done
                        } else {
                            TodoStateActionData::Dismiss
                        },
                        dry_run: false,
                    },
                )
                .await;
            }
            if decision == "fields" {
                request(
                    &fx,
                    Request::ScheduleTodo {
                        todo_id: current.id.clone(),
                        when: Some("2026-11-05T09:00:00Z".into()),
                        time_zone: Some("UTC".into()),
                        dry_run: false,
                    },
                )
                .await;
            }
            let before = fx.state.store.get_todo(&current.id).await.unwrap().unwrap();
            finish_first_run(&fx, current.source_date.unwrap()).await;
            let after = fx.state.store.get_todo(&current.id).await.unwrap().unwrap();
            if decision == "fields" {
                assert_eq!(after, before);
            } else {
                // Automatic facts may refresh, but a saved closed decision and its history stay authoritative.
                assert_eq!(after.id, before.id);
                assert_eq!(after.state, before.state);
                assert_eq!(after.done_at, before.done_at);
                assert_eq!(after.dismissed_at, before.dismissed_at);
                assert_eq!(after.due_at, before.due_at);
                assert_eq!(after.reason, before.reason);
                assert_eq!(after.dedup_key, before.dedup_key);
                assert!(!runway(&fx, current.source_date.unwrap())
                    .await
                    .now
                    .iter()
                    .any(|t| t.id == after.id));
            }
            if decision == "fields" {
                assert!(before.scheduled_for.is_some());
                assert_eq!(before.kind, "lease");
                assert_eq!(before.amount_minor, Some(14000));
                assert_ne!(before.due_at, original.due_at);
            }
        }
        let fx = Fixture::new().await;
        let (id, current) = bank_journey(&fx).await;
        seed_published_row(&fx, &current).await;
        let original = stored(&fx, &id).await;
        let ResponseData::TodoChange { change } = request(
            &fx,
            Request::CorrectTodo {
                todo_id: current.id.clone(),
                reason: mxr_protocol::TodoCorrectionData::WrongDetails,
                edits: vec![mxr_protocol::TodoEditData {
                    field: "title".into(),
                    value: "My bank payment".into(),
                }],
                time_zone: None,
                dry_run: false,
            },
        )
        .await
        else {
            panic!("correct")
        };
        let corrected = stored(&fx, &id).await;
        finish_first_run(&fx, current.source_date.unwrap()).await;
        assert_eq!(stored(&fx, &id).await, corrected);
        request(
            &fx,
            Request::UndoTodoCorrection {
                todo_id: current.id,
                correction_id: change.correction_undo.unwrap(),
                dry_run: false,
            },
        )
        .await;
        assert_eq!(stored(&fx, &id).await, original);
    });
}
