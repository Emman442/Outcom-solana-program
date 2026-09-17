# Outcom (Solana)

Solana program for Outcom work trials.

Employers lock USDC in a per-trial vault. Candidates start a trial. Referrers attach a candidate on-chain. GenLayer verifies submitted evidence and a LayerZero message settles the vault: candidate + referrer get paid on PASS, or the trial is marked rejected on FAIL.

Program ID:DMbLxuGRQdtYwhsXTGdp1qAKbzzR7jiR3gvvttgU36Tj


## What it does

1. `initialize_trial` — create the trial PDA, create the USDC vault, pull `candidate_reward + referral_reward` from the employer ATA.
2. `start_trial` — first candidate to call it becomes `selected_candidate` and status moves to `InProgress`.
3. `refer_candidate` — store a referral PDA keyed by trial + candidate.
4. `lz_receive` — LayerZero endpoint delivers a GenLayer verdict payload. If `is_verified` and `score >= 70`, transfer rewards from the vault. Otherwise set `Rejected`.

OApp helpers (`init_oapp`, `set_peer`) register the LayerZero endpoint and allowed peer.

## Account seeds

| Account | Seeds |
|---|---|
| `OAppConfig` | `["OApp"]` |
| `PeerConfig` | `["Peer", oapp_config, src_eid_be_bytes]` |
| `TrialAccount` | `["Trial", employer, trial_id_bytes]` |
| Vault ATA | `["Vault", trial_account]` |
| `Referral` | `["Referral", trial_account, candidate]` |

`trial_id` max length is 32 bytes.

## Trial account

On-chain fields: employer, trial_id, title, description, category, skills, difficulty, objective, requirements, candidate_reward, referral_reward, total_locked, status, selected_candidate, vault_bump.

Status enum:Open → InProgress → UnderReview → Paid
                               Rejected



`ReadyTorun` and `Verified` exist on the enum but are not written by the current instructions.

## Instructions

### `init_oapp(endpoint_program, admin)`

Creates the OApp config. Admin is the only signer allowed to `set_peer`.

### `set_peer(src_eid, peer_address)`

Whitelist a LayerZero source EID + 32-byte peer. Incoming `lz_receive` must match this peer.

### `initialize_trial(...)`

Args: `trial_id`, `title`, `description`, `category`, `skills`, `difficulty`, `objective`, `requirements`, `candidate_reward`, `referral_reward`.

Accounts: employer (signer), trial PDA, vault ATA, USDC mint, employer USDC ATA, token program.

Length caps:

- title 64
- description 256
- category 32
- skills 128
- difficulty 16
- objective 256
- requirements 512

### `start_trial`

Accounts: candidate (signer), trial account.

Requires `status == Open`. Sets `selected_candidate` and `InProgress`.

### `refer_candidate(candidate, note)`

Accounts: referrer (signer), trial account, referral PDA.

Allowed while `Open` or `InProgress`. Referrer cannot equal candidate. Note max 128 chars. One referral PDA per (trial, candidate).

### `lz_receive(params)`

Called by the LayerZero endpoint program.

Checks:

- caller is `oapp_config.endpoint_program`
- `params.src_eid` matches peer
- `params.sender` matches peer address
- payload length ≥ 105
- payload `trial_id` matches the trial account
- status is `InProgress` or `UnderReview`
- candidate ATA owner matches payload candidate
- if a referrer ATA is passed, its owner matches payload referrer

On PASS (`is_verified && score >= 70`):

- transfer `candidate_reward` from vault → candidate ATA
- if `referral_reward > 0` and referrer ATA is present, transfer that too
- status → `Paid`

On FAIL: status → `Rejected`. Vault is not refunded in this instruction.

## GenLayer payload (105+ bytes)

Encoded by the GenLayer `OutcomVerifier` contract and delivered over LayerZero:

| Offset | Size | Field |
|---|---|---|
| 0 | 32 | `trial_id` UTF-8, null-padded |
| 32 | 32 | candidate pubkey |
| 64 | 32 | referrer pubkey (32 zero bytes if none) |
| 96 | 8 | score, big-endian u64 |
| 104 | 1 | passed flag (`0x01` / `0x00`) |

Score `80` on PASS, `0` on FAIL in the current GenLayer contract.

## Build / deploy

```bash
anchor build
anchor deploy