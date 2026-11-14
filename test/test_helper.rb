# frozen_string_literal: true
require 'minitest/autorun'
require 'consensus_machines'

module RaftHelpers
  include ConsensusMachines

  EMPTY = ConsensusMachines::Position.new(0, 0)

  def node(count = 3, own = 0)
    Raft.new(members(count), own)
  end

  def members(count = 3) = (10...(10 + count)).to_a

  def granted(term) = VoteResponse.new(term, :granted)

  def matched(term, index) = AppendResponse.new(term, Matched.new(index))

  # Seat 0, elected in term 1 by a bare majority.
  def leader(count = 3)
    raft = node(count)
    raft.begin_election
    (1..(count / 2)).each { |peer| raft.receive_vote(peer, granted(1)) }
    assert_equal :leader, raft.role
    raft
  end

  # The call answers `refusal` and the server is exactly as it was.
  def assert_refused(refusal, raft)
    before = [raft.checkpoint, raft.role]
    assert_equal refusal, yield(raft)
    assert_equal before, [raft.checkpoint, raft.role], "#{refusal} changed the server"
  end

  # The call is honoured and the server is exactly as it was.
  def assert_unchanged(raft)
    before = [raft.checkpoint, raft.role]
    answer = yield(raft)
    refute_kind_of Symbol, answer
    assert_equal before, [raft.checkpoint, raft.role], 'an honoured repeat changed the server'
    answer
  end
end

# Runs a hook once in place of the next `new` on a class, before the real
# one: an answer class whose construction raises, re-enters or freezes.
module AnswerHook
  HOOKS = {} # rubocop:disable Style/MutableConstant

  def new(...)
    AnswerHook::HOOKS.delete(self)&.call
    super
  end

  def self.around(klass, hook)
    klass.singleton_class.prepend(self) unless klass.singleton_class.ancestors.include?(self)
    HOOKS[klass] = hook
    yield
  ensure
    HOOKS.delete(klass)
  end
end

module RaftHelpers
  # Builds a value past `initialize`: Marshal.load skips it. `from` and `to`
  # are the marshalled bytes of one field, before and after.
  def forge(value, from, to)
    bytes = Marshal.dump(value)
    raise ArgumentError, 'field not found' unless bytes.include?(from)

    Marshal.load(bytes.sub(from, to)) # rubocop:disable Security/MarshalLoad
  end
end
