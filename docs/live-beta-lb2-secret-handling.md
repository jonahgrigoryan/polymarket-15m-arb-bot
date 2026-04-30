# Live Beta LB2 Secret Handling

Date: 2026-04-29
Phase: LB2 - Auth And Secret Handling, No Order Submission

## Backend Choice

Approved LB2 backend: environment-variable handles managed outside the repository.

Repository config stores only handle names:

```text
P15M_LIVE_BETA_CLOB_L2_ACCESS
P15M_LIVE_BETA_CLOB_L2_CREDENTIAL
P15M_LIVE_BETA_CLOB_L2_PASSPHRASE
```

The repo must not store credential values. LB2 validation may check whether these handles are present in the process environment, but it must not print, persist, derive from, sign with, or transmit their values.

## Access Control

- Restrict handle values to the approved deployment host and operator account.
- Do not expose handle values through shell history, committed `.env` files, config examples, logs, reports, CI, or chat.
- Paper and replay modes must run without these handles.
- Any future expansion beyond handle presence requires the next approved phase.

## Rotation

- Rotate all three handles before the first live beta if they were ever exposed outside the approved host.
- Rotate after any operator access change.
- Rotate after any failed or ambiguous auth-state incident.
- Record rotation time, operator, and reason in a local deployment note outside the repository.

## Revocation

- Revoke the handles immediately if logs, reports, shell history, CI, or chat may contain values.
- Revoke on host compromise, account compromise, or geoblock/compliance ambiguity.
- Do not restart any live-beta phase until revocation and replacement are documented.

## Audit Logging

LB2 may log:

- backend name,
- handle labels,
- handle names,
- presence booleans,
- validation status.

LB2 must not log:

- handle values,
- derived credentials,
- signed payloads,
- authenticated request headers,
- order, cancel, or readback responses.

## Deployment Setup

1. Configure the three handle values in the deployment environment outside the repo.
2. Run local config validation without handles to confirm paper/replay remain usable.
3. Run secret-handle validation only on the approved host or a controlled local shell.
4. Confirm output contains handle names and presence booleans only.
5. Preserve `LIVE_ORDER_PLACEMENT_ENABLED=false`.

LB2 does not authorize signing, authenticated clients, order posting, canceling, readback clients, wallet private-key handling, or live trading.
