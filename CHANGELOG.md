# Changelog

## [0.4.2](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.4.1...consensus_machines/v0.4.2) (2026-11-14)


### Bug Fixes

* **deps:** require matryona-machines &gt;= 0.4 and read votes, appends and responses through it ([3157989](https://github.com/seuros/consensus_machines/commit/31579897a242e848beb53b3093c80565364587cb))

## [0.4.1](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.4.0...consensus_machines/v0.4.1) (2026-11-14)


### Bug Fixes

* **deps:** require matryona-machines &gt;= 0.3, adopt Symbols::variant, names_only tables and which ([25c7ee4](https://github.com/seuros/consensus_machines/commit/25c7ee40136589abae5978f55b620039c6ffb60c))

## [0.4.0](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.3.0...consensus_machines/v0.4.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona-machines 0.2 (NameError at load, exact_fields/exact_array, symbols!, frozen-first)

### Features

* adopt matryona-machines 0.2 (NameError at load, exact_fields/exact_array, symbols!, frozen-first) ([a52ed78](https://github.com/seuros/consensus_machines/commit/a52ed78f0c88467e986c46e9f38a49dd197dce3e))

## [0.3.0](https://github.com/seuros/consensus_machines/compare/consensus_machines/v0.2.0...consensus_machines/v0.3.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* refuse impossible replies as Malformed before term adoption, refuse term-zero votes, native message reads, clone-then-keep answers

### Features

* refuse impossible replies as Malformed before term adoption, refuse term-zero votes, native message reads, clone-then-keep answers ([ee400ea](https://github.com/seuros/consensus_machines/commit/ee400ea302c56e466523161e9e95819ee8b4dd82))

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
