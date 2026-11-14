# frozen_string_literal: true
require_relative 'test_helper'

# Building an answer runs Ruby (`new`), so a decision is held on a copy and
# kept only once its answer exists: whatever happens in between, a raise
# means nothing was applied.
class KeepTest < Minitest::Test
  include RaftHelpers

  def snapshot(raft) = [raft.checkpoint, raft.role]

  def test_an_answer_whose_new_raises_applies_nothing
    raft = node
    [VoteRequest, Position].each do |answer|
      before = snapshot(raft)
      AnswerHook.around(answer, -> { raise IOError, 'interrupted' }) { assert_raises(IOError) { raft.begin_election } }
      assert_equal before, snapshot(raft)
    end
    follower = node(3, 1)
    [Received, VoteResponse].each do |answer|
      before = snapshot(follower)
      AnswerHook.around(answer, -> { raise IOError }) do
        assert_raises(IOError) { follower.request_vote(VoteRequest.new(0, 1, EMPTY)) }
      end
      assert_equal before, snapshot(follower)
    end
    [AppendResponse, Matched].each do |answer|
      before = snapshot(follower)
      AnswerHook.around(answer, -> { raise IOError }) do
        assert_raises(IOError) { follower.append_entries(AppendRequest.new(0, 1, EMPTY, 0), [Entry.new(1, 5)]) }
      end
      assert_equal before, snapshot(follower)
    end
  end

  def test_receive_vote_and_acknowledge_keep_their_copy
    raft = node
    raft.begin_election
    before = snapshot(raft)
    AnswerHook.around(Received, -> { raise IOError }) { assert_raises(IOError) { raft.receive_vote(1, granted(1)) } }
    assert_equal before, snapshot(raft)
    chief = leader
    chief.propose(7)
    before = snapshot(chief)
    AnswerHook.around(Received, -> { raise IOError }) { assert_raises(IOError) { chief.acknowledge(1, matched(1, 1)) } }
    assert_equal before, snapshot(chief)
    assert_equal :advanced, chief.acknowledge(1, matched(1, 1)).outcome
  end

  def test_an_answer_whose_new_re_enters_applies_only_the_inner_call
    raft = node
    inner = nil
    AnswerHook.around(VoteRequest, -> { inner = raft.begin_election }) do
      assert_raises(RuntimeError) { raft.begin_election }
    end
    assert_equal VoteRequest.new(0, 1, EMPTY), inner
    assert_equal [1, :candidate], [raft.term, raft.role]
  end

  def test_an_answer_whose_new_freezes_the_receiver_applies_nothing
    raft = node
    before = snapshot(raft)
    AnswerHook.around(VoteRequest, -> { raft.freeze }) { assert_raises(FrozenError) { raft.begin_election } }
    assert_equal before, snapshot(raft)
    assert_raises(FrozenError) { raft.propose(1) }
  end

  # The binding holds its classes by address: they are pinned, so compaction
  # moves none of them and every path that checks or builds one still works.
  def test_classes_survive_compaction
    GC.compact if GC.respond_to?(:compact)
    raft = node
    assert_equal VoteRequest.new(0, 1, EMPTY), raft.begin_election
    assert_equal :elected, raft.receive_vote(1, granted(1)).outcome
    assert_equal 1, raft.propose(7)
    assert_equal :advanced, raft.acknowledge(1, matched(1, 1)).outcome
    follower = node(3, 1)
    assert_instance_of VoteResponse, follower.request_vote(VoteRequest.new(0, 1, EMPTY)).outcome
    answer = follower.append_entries(AppendRequest.new(0, 1, EMPTY, 0), [Entry.new(1, 7)])
    assert_equal Matched.new(1), answer.outcome.append
    assert_equal follower.checkpoint, Raft.restored(members, 1, follower.checkpoint).checkpoint
  end
end
