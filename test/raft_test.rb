# frozen_string_literal: true
require_relative 'test_helper'
class RaftTest < Minitest::Test
  def node(count = 3, own = 0)
    ConsensusMachines::Raft.new((10...(10 + count)).to_a, own)
  end

  def test_exact_membership_majority_for_every_supported_size
    (1..9).each do |count|
      raft = node(count)
      assert_equal [0, 1, 0, 0], raft.begin_election
      majority = count / 2 + 1
      (1...majority).each do |peer|
        assert_equal 'candidate', raft.role
        raft.receive_vote(peer, 1, 1)
      end
      assert_equal 'leader', raft.role, "exact quorum for #{count} actual peers"
      assert_equal 1, raft.propose(42)
      assert_equal(count == 1 ? 1 : 0, raft.committed)
      (1...majority).each { |peer| assert raft.acknowledge(peer, 1, 1) }
      assert_equal 1, raft.committed
      assert_equal [1, 42], raft.entry(1)
    end
  end

  def test_distinct_votes_terms_append_and_checkpoint
    assert_raises(TypeError) { ConsensusMachines::Checkpoint.new }
    assert_raises(TypeError) { ConsensusMachines::Raft.allocate }
    leader = node(5)
    leader.begin_election
    leader.receive_vote(1, 1, 1)
    leader.receive_vote(1, 1, 1)
    assert_equal 'candidate', leader.role
    leader.receive_vote(2, 1, 1)
    assert_equal 'leader', leader.role
    leader.propose(99)
    follower = node(5, 1)
    assert_equal [1, true], follower.request_vote(0, 1, 0, 0)
    assert_equal [1, false], follower.request_vote(2, 1, 0, 0)
    assert_equal [1, 1], follower.append_entries(0, 1, 0, 0, 0, [[1, 99]])
    assert_equal [1, 99], follower.entry(1)
    assert_equal [1, nil], follower.append_entries(0, 1, 1, 2, 0, [])
    assert_equal [1, 1], follower.append_entries(0, 1, 1, 1, 1, [])
    assert_equal 1, follower.committed
    saved = leader.checkpoint
    assert_raises(TypeError) { saved.dup }
    assert_raises(ArgumentError) { follower.restore!(saved) }
    GC.start
    GC.compact
    assert leader.restore!(saved)
    assert_equal 'follower', leader.role
    assert_equal [1, 1], leader.last
    assert_raises(ArgumentError) { leader.restore!(saved) }
    stale = follower.checkpoint
    follower.begin_election
    assert_raises(ArgumentError) { follower.restore!(stale) }
    leader.receive_vote(1, 10, 0)
    assert_equal 10, leader.term
  end

  def test_validation_precedes_mutation_and_all_frozen_guards
    assert_raises(ArgumentError) { node(0) }
    assert_raises(ArgumentError) { node(10) }
    assert_raises(ArgumentError) { ConsensusMachines::Raft.new([1, 1], 0) }
    assert_raises(ArgumentError) { node(3, 3) }
    assert_raises(TypeError) { ConsensusMachines::Raft.new([1.1], 0) }
    raft = node
    saved = raft.checkpoint
    assert_raises(TypeError) { raft.propose(1.5) }
    assert_raises(RangeError) { raft.propose(-1) }
    assert_raises(RangeError) { raft.propose(2**80) }
    assert_raises(ArgumentError) { raft.receive_vote(1, 1, 2) }
    assert_raises(TypeError) { raft.append_entries(1, 10, 0, 0, 0, [[10, 1], [10, 'bad']]) }
    assert_raises(ArgumentError) { raft.append_entries(1, 10, 0, 0, 0, [[10]]) }
    assert_equal 0, raft.term
    assert raft.restore!(saved)
    saved = raft.checkpoint
    raft.freeze
    assert_raises(FrozenError) { raft.begin_election }
    assert_raises(FrozenError) { raft.request_vote(1, 1, 0, 0) }
    assert_raises(FrozenError) { raft.receive_vote(1, 1, 1) }
    assert_raises(FrozenError) { raft.propose(1) }
    assert_raises(FrozenError) { raft.append_entries(1, 1, 0, 0, 0, []) }
    assert_raises(FrozenError) { raft.acknowledge(1, 1, 1) }
    assert_raises(FrozenError) { raft.restore!(saved) }
    assert_equal [0, 0], raft.last
  end

  def test_bounded_log_and_rejection_of_uncommitted_application
    raft = node(1)
    assert_nil raft.propose(1)
    raft.begin_election
    64.times { |i| assert_equal i + 1, raft.propose(i) }
    assert_nil raft.propose(65)
    assert_nil raft.entry(65)
    assert_equal 64, raft.committed
  end

  def test_nested_entry_coercion_cannot_grow_shrink_or_freeze_during_decode
    %i[grow_past_capacity grow_within_capacity shrink freeze].each do |attack|
      raft = node
      saved = raft.checkpoint
      entries = []
      called = false
      coercible = Object.new
      coercible.define_singleton_method(:to_ary) do
        called = true
        case attack
        when :grow_past_capacity then entries.concat(Array.new(65) { [1, 99] })
        when :grow_within_capacity then entries << [1, 99]
        when :shrink then entries.clear
        when :freeze then raft.freeze
        end
        [1, 99]
      end
      entries << coercible
      assert_raises(TypeError) { raft.append_entries(1, 1, 0, 0, 0, entries) }
      refute called, "must not invoke #{attack} to_ary callback"
      assert_equal [coercible], entries
      refute raft.frozen?
      assert_equal 0, raft.term
      assert_equal [0, 0], raft.last
      assert raft.restore!(saved), 'failed decoding must leave the revision unchanged'
    end

    raft = node
    assert_equal [1, 2], raft.append_entries(1, 1, 0, 0, 0, [[1, 41], [1, 42]])
    assert_equal [1, 41], raft.entry(1)
    assert_equal [1, 42], raft.entry(2)
    assert_equal [2, 1], raft.last
  end
end
