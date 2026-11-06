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
A full fixed-size log rejects proposals; no compaction/snapshot installation yet.

## Mandatory external safety contract

- Serialize node transitions and authenticate/correlate all messages to the same
  cluster, configuration, term, and replication request. Quorum arithmetic alone
  does not prove successful replication.
- **Durably commit `checkpoint()` after every state transition before exposing
  its replies, broadcasting requests/entries, counting local persistence, or
  applying committed commands.** The API returns proposed in-memory decisions;
  it does not provide a persistence barrier.
- On storage uncertainty, stop and recover the latest committed checkpoint.
  Never roll back terms, votes, or durable log history.
- Restore as follower. Keep timers, leader retry/next-index logic, heartbeat
  scheduling, command-data storage, applied-index recovery, and client
  deduplication in a tested runtime. Apply only committed commands, in order,
  with an explicit crash-replay contract.

Typed checkpoints are opaque state transfers, not a stable disk/wire format.
No transport, election timeout randomization, pre-vote, leader lease/read-index,
linearizable read API, snapshotting, Byzantine protection, production liveness
claim, or Ruby binding is implemented. Do not deploy this model as a complete
distributed consensus service.

```rust
use consensus_machines::{Raft, Role};
# fn example() -> Option<()> {
// A local single-member model, not a storage or transport implementation.
let mut node = Raft::<1, 8>::new([1], 0)?;
node.begin_election()?;
assert_eq!(node.role(), Role::Leader);
assert_eq!(node.propose(42), Some(1));
assert_eq!(node.committed(), 1);
let recovered = Raft::restore(node.checkpoint())?;
assert_eq!(recovered.role(), Role::Follower);
# Some(())
# }
# assert_eq!(example(), Some(()));
```
