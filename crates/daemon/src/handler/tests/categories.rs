use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::ThreadId;

#[tokio::test]
async fn multi_message_multi_category_preview_apply_and_undo_use_same_cartesian_selection() {
    let fx = Fixture::new().await;
    let category_a = fx
        .state
        .store
        .create_personal_category("Travel")
        .await
        .unwrap();
    let category_b = fx
        .state
        .store
        .create_personal_category("Home")
        .await
        .unwrap();
    let first = fx
        .message(
            &ThreadId::new(),
            "one@example.com",
            ME,
            Duration::days(1),
            None,
        )
        .await;
    let second = fx
        .message(
            &ThreadId::new(),
            "two@example.com",
            ME,
            Duration::days(2),
            None,
        )
        .await;
    let account_id = Some(fx.account.clone());
    let message_ids = vec![first.id.clone(), second.id.clone()];
    let category_ids = vec![category_a.id.clone(), category_b.id.clone()];

    let ResponseData::CategoryMembershipPreview { preview } = request(
        &fx,
        Request::PreviewCategoryMembership {
            account_id: account_id.clone(),
            message_ids: message_ids.clone(),
            add_category_ids: category_ids.clone(),
            remove_category_ids: vec![],
        },
    )
    .await
    else {
        panic!("expected membership preview")
    };
    assert_eq!(preview.added, 4);
    let ResponseData::CategoryMembershipApplied {
        mutation_id,
        added,
        removed,
    } = request(
        &fx,
        Request::ApplyCategoryMembership {
            preview_token: preview.preview_token,
        },
    )
    .await
    else {
        panic!("expected membership apply")
    };
    assert_eq!((added, removed), (4, 0));

    let ResponseData::CategoryMembershipUndone { restored, skipped } =
        request(&fx, Request::UndoCategoryMembership { mutation_id }).await
    else {
        panic!("expected membership undo")
    };
    assert_eq!((restored, skipped), (4, 0));
    let ResponseData::CategoryMembershipStates { states } = request(
        &fx,
        Request::GetCategoryMemberships {
            account_id,
            message_ids,
            category_ids,
        },
    )
    .await
    else {
        panic!("expected membership states")
    };
    assert!(states.iter().all(|state| state.assigned_messages == 0));
}
