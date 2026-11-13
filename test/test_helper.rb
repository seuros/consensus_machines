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
