# frozen_string_literal: true

require_relative 'consensus_machines/version'

# Every value mirrors the core type of the same name, field for field. Terms
# and log indices are protocol numbers the members must agree on, so they are
# plain Integers.
#
# The native side is the authority on every value it reads: it must be
# exactly this gem's class (a subclass is a TypeError, whatever it holds), and
# its fields are validated again through the core's constructor, so a message
# forged past `initialize` (`Marshal.load`, a bound `Data#initialize`) raises
# TypeError or RangeError and never reaches the server: it can never earn
# :malformed.
module ConsensusMachines
  # Asks the core's validating constructor through a private hook: TypeError
  # for a wrong type, RangeError for a value the core rejects.
  def self.check(hook, *fields) = __send__(hook, *fields)
  private_class_method :check

  # A log position; index and term are both zero (the empty prefix) or both
  # set. The core's `Position::new`.
  Position = Data.define(:index, :term) do
    def initialize(index:, term:)
      ConsensusMachines.__send__(:check, :__position, index, term)
      super
    end
  end

  # One log entry: the term of the leader that created it (positive) and the
  # Integer id of a command the caller stores. The core's `Entry::new`.
  Entry = Data.define(:term, :command) do
    def initialize(term:, command:)
      ConsensusMachines.__send__(:check, :__entry, term, command)
      super
    end
  end

  # RequestVote: what #begin_election answers and #request_vote is given. The
  # core's `VoteRequest::new`: a positive term, not below `last`'s.
  VoteRequest = Data.define(:candidate, :term, :last) do
    def initialize(candidate:, term:, last:)
      ConsensusMachines.__send__(:check, :__vote_request, candidate, term, last)
      super
    end
  end

  # VoteResponse#vote when the vote is kept from the candidate. `reason` is
  # :term_behind, :already_voted or :log_behind.
  Denied = Data.define(:reason) do
    def initialize(reason:)
      ConsensusMachines.__send__(:check, :__denied, reason)
      super
    end
  end

  # The reply to a RequestVote. `vote` is :granted, :duplicate (this candidate
  # already had the vote; counts like a grant) or a Denied. The core's
  # `VoteResponse::new`: a positive term, above one for :term_behind.
  VoteResponse = Data.define(:term, :vote) do
    def initialize(term:, vote:)
      ConsensusMachines.__send__(:check, :__vote_response, term, vote)
      super
    end
  end

  # AppendEntries metadata; the entries travel beside it. The core's
  # `AppendRequest::new`: a positive term, not below `previous`'s.
  AppendRequest = Data.define(:leader, :term, :previous, :committed) do
    def initialize(leader:, term:, previous:, committed:)
      ConsensusMachines.__send__(:check, :__append_request, leader, term, previous, committed)
      super
    end
  end

  # AppendResponse#append when the log agrees with the leader's up to `index`.
  Matched = Data.define(:index) do
    def initialize(index:)
      ConsensusMachines.__send__(:check, :__matched, index)
      super
    end
  end

  # The reply to an AppendEntries. `append` is a Matched, :conflict or
  # :term_behind. The core's `AppendResponse::new`: a positive term, above
  # one for :term_behind.
  AppendResponse = Data.define(:term, :append) do
    def initialize(term:, append:)
      ConsensusMachines.__send__(:check, :__append_response, term, append)
      super
    end
  end

  # What an honoured inbound message did, in two stages. `adopted` is the
  # higher term this call moved the server into (vote cleared, now a
  # follower), or nil when the term was kept. `role` is the role after the
  # call. `outcome` is what the message itself came to. The core's
  # `Received::new`: only an answer a server can give exists.
  Received = Data.define(:adopted, :role, :outcome) do
    def initialize(adopted:, role:, outcome:)
      ConsensusMachines.__send__(:check, :__received, adopted, role, outcome)
      super
    end
  end

  # A server's durable state, as plain data to persist however you like.
  # `entries` is an Array of Entry. The core's `Checkpoint::new`, for
  # Raft::MEMBERS members and at most Raft::CAPACITY entries; both Arrays are stored as frozen copies.
  Checkpoint = Data.define(:members, :own, :term, :voted_for, :committed, :entries) do
    def initialize(members:, own:, term:, voted_for:, committed:, entries:)
      raise TypeError, "members must be an Array, not #{members.class}" unless members.is_a?(Array)
      raise TypeError, "entries must be an Array, not #{entries.class}" unless entries.is_a?(Array)

      members = members.dup.freeze
      entries = entries.dup.freeze
      ConsensusMachines.__send__(:check, :__checkpoint, members, own, term, voted_for, committed, entries)
      super
    end
  end

  # Sealed: no subclass can skip the checks above by overriding `initialize`,
  # and no bare `allocate` builds a value that never ran them. Ruby still
  # binds a subclass constant before `inherited` raises; the native side
  # refuses its instances.
  [Position, Entry, VoteRequest, Denied, VoteResponse, AppendRequest, Matched, AppendResponse, Received,
   Checkpoint].each do |value|
    value.singleton_class.undef_method(:allocate)
    value.define_singleton_method(:inherited) { |_| raise TypeError, "#{value} cannot be subclassed" }
  end
end

# Native is required: no algorithm fallback. It resolves the classes above
# once, here.
require 'consensus_machines_native/consensus_machines_native'

module ConsensusMachines
  hooks = %i[__position __entry __vote_request __denied __vote_response __append_request __matched
             __append_response __received __checkpoint]
  private_class_method(*hooks)

  # The core's `Raft`, for 1..9 members and a log of CAPACITY entries.
  # `begin_election`, `request_vote`, `receive_vote`, `append_entries` and
  # `acknowledge` read every message from exactly this gem's class, decide on
  # a copy, build the answer (its `new` runs), then keep the copy only if
  # this server is still unfrozen (FrozenError) and unchanged (RuntimeError:
  # a re-entrant call changed it). Either way, a raise means nothing was
  # applied. A refusal is a Symbol.
  class Raft
    def last = Position.new(*native_last)

    def entry(index)
      found = native_entry(index)
      found && Entry.new(*found)
    end

    def checkpoint
      members, own, term, voted_for, committed, log = native_checkpoint
      entries = log.each_slice(2).map { |pair| Entry.new(*pair) }
      Checkpoint.new(members, own, term, voted_for, committed, entries)
    end
  end
end
