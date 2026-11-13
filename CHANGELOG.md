# Changelog

## [0.2.0](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.1.4...consensus_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* converted to the fleet conventions. One exhaustive Refusal per crate, Result from every operation that can refuse, a refusal changes nothing, provable idempotent repeats are honoured, Ruby mirrors the core. No compatibility aliases. Requires quorum-machines >= 0.3. Inbound calls return Result<Received<T>, Refusal>; restore is restored(members, own, checkpoint); Ruby mirrors the request and response structs as values.

### Features

* report term adoption separately from refusals and make every Raft answer explicit ([7d6513e](https://github.com/seuros/consensus_machines/commit/7d6513e3dc5ae54ff0cde572c5e6b4b3fc1168f3))

## [0.1.4](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.1.3...consensus_machines/v0.1.4) (2026-11-06)


### Bug Fixes

* extract Wrapper::advance for node revision handling ([d09a5b1](https://github.com/seuros/consensus_machines/commit/d09a5b1e6f8f7f093be7f0b3c0ddcf6188417471))

## [0.1.3](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.1.2...consensus_machines/v0.1.3) (2026-11-06)


### Bug Fixes

* crates.io keywords and categories for the core ([3eacc56](https://github.com/seuros/consensus_machines/commit/3eacc564584d1a2485c2ce150db6673ffedfc698))

## [0.1.2](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.1.1...consensus_machines/v0.1.2) (2026-11-06)


### Bug Fixes

* keep test sources out of the packaged gem and crate ([6ef97ab](https://github.com/seuros/consensus_machines/commit/6ef97abf2cde36bf051e5919c3c32f76c32d989a))

## [0.1.1](https://github.com/seuros/consensus_machines/compare/consensus_machines-v0.1.0...consensus_machines/v0.1.1) (2026-11-06)


### Features

* initial extraction ([385650b](https://github.com/seuros/consensus_machines/commit/385650b7dc025c8d4494eadb1f4c59dcad67aa97))
