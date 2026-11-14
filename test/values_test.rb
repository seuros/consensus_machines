# frozen_string_literal: true
require_relative 'test_helper'
# Values are valid or do not exist: every Data goes through the core's
# validating constructor, whether the server built it or you did.
class ValuesTest < Minitest::Test
  include RaftHelpers

  def test_hooks_are_private
    %i[__position __entry __vote_request __denied __vote_response __append_request __matched
       __append_response __received __checkpoint check].each do |hook|
      refute ConsensusMachines.respond_to?(hook), hook
    end
  end

  def test_positions_and_entries
    assert_raises(RangeError) { Position.new(0, 1) }
    assert_raises(RangeError) { Position.new(1, 0) }
    assert_raises(RangeError) { Position.new(-1, 0) }
    assert_raises(TypeError) { Position.new(1.0, 1) }
    assert_raises(RangeError) { Position.new(1, 1).with(term: 0) }
    assert_raises(RangeError) { Entry.new(0, 1) }
    assert_raises(TypeError) { Entry.new(1, nil) }
    assert_raises(ArgumentError) { Entry.new(1) }
    assert_equal Entry.new(1, 0), Entry.new(term: 1, command: 0)
  end

  def test_requests
    assert_raises(RangeError) { VoteRequest.new(0, 0, EMPTY) }
    assert_raises(RangeError) { VoteRequest.new(0, 1, Position.new(1, 2)) }
    assert_raises(TypeError) { VoteRequest.new(0, 1, [0, 0]) }
    assert_raises(RangeError) { VoteRequest.new(0, 1, EMPTY).with(term: 0) }
    assert_raises(RangeError) { AppendRequest.new(0, 0, EMPTY, 0) }
    assert_raises(RangeError) { AppendRequest.new(0, 1, Position.new(1, 2), 0) }
    assert_raises(RangeError) { AppendRequest.new(0, 1, EMPTY, -1) }
    assert_raises(TypeError) { AppendRequest.new(0, 1, nil, 0) }
  end

  def test_responses_and_their_parts
    assert_raises(RangeError) { VoteResponse.new(0, :granted) }
    assert_raises(RangeError) { VoteResponse.new(1, Denied.new(:term_behind)) }
    assert_equal 2, VoteResponse.new(2, Denied.new(:term_behind)).term
    assert_raises(RangeError) { VoteResponse.new(1, :denied) }
    assert_raises(TypeError) { VoteResponse.new(1, 'granted') }
    assert_raises(RangeError) { Denied.new(:sulking) }
    assert_raises(TypeError) { Denied.new('log_behind') }
    assert_raises(RangeError) { AppendResponse.new(0, :conflict) }
    assert_raises(RangeError) { AppendResponse.new(1, :term_behind) }
    assert_raises(RangeError) { AppendResponse.new(1, :matched) }
    assert_raises(RangeError) { Matched.new(-1) }
    assert_raises(TypeError) { Matched.new(1.5) }
  end

  def test_received_holds_only_answers_a_server_gives
    assert_equal :elected, Received.new(nil, :leader, :elected).outcome
    assert_raises(RangeError) { Received.new(nil, :candidate, :elected) }
    assert_raises(RangeError) { Received.new(nil, :follower, :counted) }
    assert_raises(RangeError) { Received.new(0, :follower, :overtaken) }
    assert_raises(RangeError) { Received.new(3, :leader, :overtaken) }
    assert_raises(RangeError) { Received.new(nil, :follower, :overtaken) }
    # :duplicate names a ballot and an ack: a candidate may hold it, a follower never.
    assert_equal :duplicate, Received.new(nil, :candidate, :duplicate).outcome
    assert_raises(RangeError) { Received.new(nil, :follower, :duplicate) }
    assert_raises(RangeError) { Received.new(nil, :leader, :maybe) }
    assert_raises(RangeError) { Received.new(nil, :boss, :counted) }
    assert_raises(TypeError) { Received.new(nil, 'leader', :counted) }
    assert_raises(TypeError) { Received.new(nil, :leader, 5) }
    assert_raises(TypeError) { Received.new(1.0, :follower, :overtaken) }
    assert_raises(RangeError) { Received.new(2, :follower, granted(3)) }
    assert_raises(RangeError) { Received.new(nil, :leader, granted(3)) }
    assert_raises(RangeError) { Received.new(nil, :leader, matched(3, 1)) }
    assert_raises(RangeError) { Received.new(3, :follower, AppendResponse.new(3, :term_behind)) }
    assert_raises(RangeError) { Received.new(nil, :leader, :advanced).with(role: :candidate) }
  end

  # Whatever the server answers, its constructor accepts: rebuilding an answer
  # by hand gives an equal value, so the binding cannot raise after the core
  # has moved.
  def test_every_answer_rebuilds_to_itself
    raft = leader(3)
    answers = [raft.receive_vote(2, granted(1)), raft.acknowledge(1, matched(1, 0)),
               node(3, 1).request_vote(VoteRequest.new(0, 1, EMPTY)),
               node(3, 1).append_entries(AppendRequest.new(0, 1, EMPTY, 0), [Entry.new(1, 1)]),
               raft.receive_vote(1, VoteResponse.new(5, Denied.new(:term_behind)))]
    answers.each { |answer| assert_equal answer, Received.new(*answer.deconstruct) }
    saved = raft.checkpoint
    assert_equal saved, Checkpoint.new(*saved.deconstruct)
  end
end
