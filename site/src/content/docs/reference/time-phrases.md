---
title: Time phrases
description: The words mxr understands wherever it asks for a time, and how they resolve.
---

Every place mxr asks for a time reads the same phrases: `mxr snooze --until`,
`mxr send --at`, `mxr send --remind-after`, `mxr remind --when`,
`mxr send-time --at`, the TUI snooze and send-later prompts, and the web
app's snooze, send-later and reminder fields. One parser in the daemon
resolves them, so the CLI, TUI and web give the same answer.

Phrases resolve in your local time zone. Where a phrase leaves out the time,
mxr uses your `[snooze]` hours from [config](/reference/config/):
`morning_hour` (default 09:00), `evening_hour` (18:00), and `weekend_hour`
(10:00) on `weekend_day` (Saturday).

## Preview before you commit

`mxr time` shows what a phrase resolves to without changing anything:

```bash
mxr time fri 3
# Friday 2 October, 15:00 (in 5 days)
#   Assumed: am or pm
#   Or: Friday 2 October, 03:00 (in 5 days)

mxr time "in 2d" --format json
```

The JSON has the resolved instant (`resolution.at`), every reading in
`resolution.choices` with the default first, the byte `spans` of the input
that were understood, and what was `implied` (the day, the year, the time,
or am/pm). A phrase mxr can't read exits non-zero with a message that
names the word, such as `Didn't catch "frday". Try "fri 3pm" or "in 2d".`
Pass `--now <RFC3339>` to resolve against a fixed instant.

The web app and TUI show the same preview as you type. When a phrase has
more than one reading, they offer each as a choice (Tab in the TUI, arrow
keys, number keys or a click in the web app). Flags take the default
reading, which is the one `mxr time` lists first.

## Accepted phrases

| You type | It means |
|---|---|
| `in 30m`, `in 2h`, `in 5d`, `in 1w`, `in 2 months` | That long from now. Days, weeks and months keep the time of day. |
| `3d`, `2w`, `90min` | The same, without `in`. |
| `in an hour`, `in 2h 30m` | Words and combined offsets. |
| `tomorrow`, `tom` | Tomorrow at your morning hour. |
| `tonight` | Today at your evening hour. It has passed once the evening hour has. |
| `today 5pm` | Today at that time. `today` alone needs a time. |
| `mon` ... `sun`, `tue`, `thurs`, `friday` | The next one, never today, at your morning hour (weekend hour for Saturday and Sunday). |
| `next monday`, `this friday` | The same as the day name. |
| `weekend`, `next weekend` | Your weekend day at your weekend hour. |
| `next week` | Next Monday at your morning hour. |
| `next month` | The 1st of next month at your morning hour. |
| `end of week`, `eow` | Friday at 17:00. |
| `3 oct`, `oct 3`, `3rd october`, `oct 3 2027` | That date, this year if it is still ahead, otherwise next year. |
| `2026-10-03`, `2026-10-03 15:00` | That date, or date and time, in local time. |
| `2026-10-03T15:00:00Z` | An exact RFC3339 instant. |

Times go before or after the day, with or without `at`: `fri 3pm`,
`3pm fri`, `fri at 15:00`.

| Time | Meaning |
|---|---|
| `9am`, `9 am`, `9:30pm`, `9a` | 12-hour time. `12am` is midnight and `12pm` is noon. |
| `17:00`, `09:30` | 24-hour time. |
| `noon`, `midday`, `midnight` | 12:00 and 00:00. |
| `eod`, `end of day` | 17:00. |
| `morning`, `afternoon`, `evening`, `night` | Your morning hour, 14:00, your evening hour. |
| `3`, `fri 3` | An hour without am or pm. See below. |

A time with no day means its next occurrence: `5pm` is today if it is
still ahead, otherwise tomorrow.

## Ambiguous hours

An hour from 1 to 12 without am or pm has two readings. mxr offers both
and picks the one inside working hours (08:00 to 20:00) as the default,
so `fri 3` defaults to 15:00 and `fri 9` to 09:00. A reading that has
already passed is dropped: at 14:00, `today 9` can only mean 21:00.
Words settle it too: `tonight 9` is 21:00.

## Clock changes

A time skipped when the clocks go forward moves forward by the gap, so
01:30 on the spring change becomes 02:30. A time that happens twice when
the clocks go back takes the first. The preview says when either happens.
