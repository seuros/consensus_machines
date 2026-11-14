# consensus-machines

> **AMS Ongaro**, NCC-4204, *Ongaro*-class command vessel.

Engine-room flight manual: model a shared command log before wiring the radios.

Allocation-free `no_std` **bounded fixed-membership Raft transition model**.
It models role/term changes, one vote per term, log freshness, append matching,
uncommitted suffix replacement, distinct-voter elections, replication progress,
and majority commitment restricted to entries from the leader's current term.
Command payloads are immutable caller-defined `u64` command identities.

Indices are one-based (`0/term 0` is the empty-log sentinel). Peer indices refer
to one validated static membership. No dynamic membership or joint consensus.
A full fixed-size log refuses proposals; no compaction/snapshot installation yet.
`Raft<N, L>` holds exactly `N` members (`Raft::MEMBERS`) and at most `L` log
entries (`Raft::CAPACITY`).

## Contract

- **A refusal means not honoured, and nothing changed.** Every operation that
  can refuse answers `Result<T, Refusal>`; `T` holds only honoured outcomes.
  A refused message adopts no term, records no vote, leaves no role and
  writes no entry, even when it carries a higher term. Tests compare the
  whole server against a clone for every refusal.
- **An inbound message is handled in two stages, and the answer shows both.**
  `request_vote`, `receive_vote`, `append_entries` and `acknowledge` answer
  `Received { adopted, role, outcome }`.
  1. *Term reconciliation.* When the sender's term is higher, the server
     moves into it, clears its vote and becomes a follower, and `adopted` is
     `Some(term)`. Otherwise the term is kept and `adopted` is `None`. A lower
     term is stale: a request is answered with the server's own term, a
     response is refused `Stale`.
  2. *The message itself.* `outcome` is what Raft defines for it. It is
     independent of stage one: a vote can be denied, or an append conflict,
     after the term was adopted. That adoption is an honoured effect the
     caller must persist; it is never part of a refusal.
- **Protocol replies are outcomes** (the fleet rule of that name in
  `CONVENTIONS.md`: the one place an `Ok` payload may say "no"). A denied
  vote and a rejected append are replies Raft defines and the sender needs:
  `Vote::Denied(Denial)`, `Append::Conflict`, `Append::TermBehind`. They are
  honoured outcomes of a well-formed request, and each reply carries the
  responder's term. Folding such a reply in is honoured too:
  `Ballot::Denied` and `Ack::Conflict` record that the reply arrived and
  change nothing. A refusal is for a call that is malformed or impossible on
  this server: nothing is sent back for it. `Append::Conflict` carries no
  back-off hint; the leader's retry position is the runtime's.
- **`Mismatch` and `CommittedConflict` are an operator signal, not a protocol
  answer.** An append whose entries contradict this server's log under the
  same `(index, term)` (`Mismatch`) or would replace an entry at or below its
  commit index (`CommittedConflict`) is refused whole, and that holds when
  the append carries a *higher* term too: no term is adopted, the role and
  the vote are kept, nothing is written, and there is no `AppendResponse`
  to send, so the sender hears nothing and keeps retrying. Raft with honest
  servers and intact storage cannot produce either one: it means this
  server's log or the sender's is corrupt (lost or rewritten storage, a
  checkpoint older than the newest restored, two clusters wired together).
  The runtime must not paper over it by replying, adopting the term by hand
  or retrying: stop the server and raise it to an operator. The price is
  deliberate: the server stays in its old term rather than follow a leader
  whose log it can prove wrong, until an operator decides which log to
  trust. (An append from a *lower* term is never one of these: it is
  answered `Append::TermBehind` before its entries are looked at.)
- **Idempotent repeats are honoured**, with a value that says so, and change
  nothing: `Vote::Duplicate` (the candidate already has this voter's vote of
  the term, which is persisted, so it survives a restore),
  `Ballot::Duplicate` (the member's grant is already in this term's tally),
  `Ack::Duplicate` (a match at or below the one recorded for the member),
  and `Append::Matched` for entries the log already holds.
- **Terms and log indices are protocol numbers, not handles.** A term is
  issued by `begin_election` and an index by `propose`, but every member must
  agree on what term 7 and index 3 mean, across servers and across restarts:
  that agreement is the protocol. They therefore do not go through
  `periscope-machines`, carry no scope, alias across servers by design, and
  persist through `Checkpoint`. An index is reissued when an uncommitted
  suffix is replaced; only `(index, term)` names an entry.
- **Restarts.** `checkpoint()` is the durable state (membership, seat, term,
  vote, commit index, log) as a plain value with readers;
  `Checkpoint::new` rebuilds one from persisted fields and is `None` for a
  state no server can be in. `Raft::restored(members, own, checkpoint)` takes
  the membership and seat explicitly and is `None` for a checkpoint of
  another membership or seat. The role, the tally and replication progress
  are volatile: a restored server is a follower. Whether a checkpoint is the
  *newest* one of its server is the caller's persistence contract; restoring
  an older one can forget a vote, and the model cannot detect it.
- **Values are valid or are built by hand.** The message and log structs keep
  public fields for reading and matching, and each has a validating
  constructor that is the one definition of a value a server can send:
  `Position::new` (index and term both zero or both set), `Entry::new`
  (positive term), `VoteRequest::new` and `AppendRequest::new` (positive
  term, not below the position's), `VoteResponse::new` and
  `AppendResponse::new` (positive term, above one for a `TermBehind`
  reply), and `Received::new` (an adopted term is the reply's, never zero,
  and leaves a follower; the outcome can follow that role, judged by the
  `Outcome` trait each outcome type implements). Each answers `None`
  otherwise. Inbound requests and replies are judged by these same
  constructors (`Malformed`), a reply before its term is looked at, so an
  impossible reply never moves the server into its term; and every answer the server gives passes them; the
  randomized test checks it on every honoured call. A binding builds its
  values through them.
- **No unreachable arms.** No `panic!`, `assert!`, `unreachable!` or
  `expect`. The log stores entries densely, so committing walks a slice and
  cannot meet a missing entry. One arm remains that the types demand: the
  quorum tracker's `UnknownMember` when counting a seat already checked
  against the membership. It is a `debug_assert!(false, ..)` answered as
  `Refusal::UnknownMember` before anything changed.
- **No time.** Terms and indices are logical. Election timeouts and
  heartbeats are the runtime's. No call takes or records a clock, so no
  answer depends on an instant.
- **Exhaustive.** No enum is `#[non_exhaustive]`; a new variant is a breaking
  change every caller handles.

## Every answer

Checks run in the order listed. Every refusal is decided before stage one, so
none of them adopts a term. "Stale request" is `term < own term`.

| Call | Refusals, before any change | Stage 1: term | Stage 2: honoured outcome | Records the clock |
| --- | --- | --- | --- | --- |
| `begin_election()` | `TermExhausted` | issues `term + 1`, votes for itself | `VoteRequest`; a single member leads at once | no |
| `request_vote(request)` | `UnknownMember`, `OwnSeat` (candidate); `Malformed` (fails `VoteRequest::new`) | stale: kept. higher: adopted | `Vote::Granted`; `Vote::Duplicate` (repeat from the candidate already voted for); `Vote::Denied(TermBehind)` (stale, nothing changed); `Vote::Denied(AlreadyVoted)`; `Vote::Denied(LogBehind)` | no |
| `receive_vote(peer, response)` | `UnknownMember`, `OwnSeat` (peer); `Malformed` (fails `VoteResponse::new`, judged before the term); `Stale` (lower term, or `Denied(TermBehind)` not from a higher one); `NotCandidate` (current term, follower) | higher: adopted | `Ballot::Counted`; `Ballot::Elected`; `Ballot::Duplicate` (repeat); `Ballot::Denied`; `Ballot::Overtaken` (exactly when adopted) | no |
| `propose(command)` | `NotLeader`, `LogFull` | none | the new index | no |
| `append_entries(request, entries)` | `UnknownMember`, `OwnSeat` (leader); `Malformed` (fails `AppendRequest::new`, an entry fails `Entry::new`, is above the request's term, or entry terms decrease); `TooManyEntries`; then, unless stale: `RivalLeader` (same term as the one this server leads), `Mismatch`, `CommittedConflict` | stale: kept. higher: adopted. same term: a candidate becomes a follower | `Append::Matched(index)` (also the repeat: entries already held); `Append::Conflict` (log unchanged); `Append::TermBehind` (stale, nothing changed) | no |
| `acknowledge(peer, response)` | `UnknownMember`, `OwnSeat` (peer); `Malformed` (fails `AppendResponse::new`, judged before the term); `Stale` (lower term, or `TermBehind` not from a higher one); `NotLeader` (current term, not leading); `BeyondLog` | higher: adopted | `Ack::Advanced`; `Ack::Duplicate` (repeat: at or below the recorded match); `Ack::Conflict` (nothing changed); `Ack::Overtaken` (exactly when adopted) | no |

`Raft::new` and `Raft::restored` answer `Option`; `checkpoint`, `members`,
`own`, `role`, `term`, `committed`, `last` and `entry` are pure reads.

A current-term `Append::Conflict` at a committed index is answered as a
conflict, not a refusal: the log is unchanged either way. `Mismatch` and
`CommittedConflict` mean the sender violated Raft (two commands under one
`(index, term)`, or a leader missing a committed entry) or this server's own
log is corrupt; the server keeps its log, does not even adopt the sender's
term, and has no reply to send. See the Contract: it is an operator signal.

## Mandatory external safety contract

- Serialize node transitions and authenticate/correlate all messages to the same
  cluster, configuration, term, and replication request. Quorum arithmetic alone
  does not prove successful replication.
- **Durably commit `checkpoint()` after every honoured transition before
  exposing its replies, broadcasting requests/entries, counting local
  persistence, or applying committed commands.** That includes an answer whose
  `adopted` is `Some`, whatever its outcome. The API returns proposed
  in-memory decisions; it does not provide a persistence barrier.
- On storage uncertainty, stop and recover the latest committed checkpoint.
  Never roll back terms, votes, or durable log history.
- Restore as follower. Keep timers, leader retry/next-index logic, heartbeat
  scheduling, command-data storage, applied-index recovery, and client
  deduplication in a tested runtime. Apply only committed commands, in order,
  with an explicit crash-replay contract.

No transport, election timeout randomization, pre-vote, leader lease/read-index,
linearizable read API, snapshotting, Byzantine protection or production
liveness claim is implemented. Do not deploy this model as a complete
distributed consensus service.

```rust
use consensus_machines::{Raft, Refusal, Role};
# fn example() -> Option<()> {
// A local single-member model, not a storage or transport implementation.
let mut node = Raft::<1, 8>::new([1], 0)?;
node.begin_election().ok()?;
assert_eq!(node.role(), Role::Leader);
assert_eq!(node.propose(42), Ok(1));
assert_eq!(node.committed(), 1);
let mut recovered = Raft::restored([1], 0, node.checkpoint())?;
assert_eq!(recovered.role(), Role::Follower);
assert_eq!(recovered.propose(43), Err(Refusal::NotLeader));
# Some(())
# }
# assert_eq!(example(), Some(()));
```
