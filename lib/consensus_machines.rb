# frozen_string_literal: true

require_relative 'consensus_machines/version'
# Native is required: no algorithm fallback.
require 'consensus_machines_native/consensus_machines_native'

# Every value mirrors the core type of the same name, field for field. Terms
# and log indices are protocol numbers the members must agree on, so they are
# plain Integers.
module ConsensusMachines
  # A log position; index zero has term zero.
  Position = Data.define(:index, :term)

  # One log entry: the term of the leader that created it and the Integer id
  # of a command the caller stores.
  Entry = Data.define(:term, :command)

  # RequestVote: what #begin_election answers and #request_vote is given.
  VoteRequest = Data.define(:candidate, :term, :last)

  # The reply to a RequestVote. `vote` is :granted, :duplicate (this candidate
  # already had the vote; counts like a grant) or a Denied.
  VoteResponse = Data.define(:term, :vote)

  # VoteResponse#vote when the vote is kept from the candidate. `reason` is
  # :term_behind, :already_voted or :log_behind.
  Denied = Data.define(:reason)

  # AppendEntries metadata; the entries travel beside it.
  AppendRequest = Data.define(:leader, :term, :previous, :committed)

  # The reply to an AppendEntries. `append` is a Matched, :conflict or
  # :term_behind.
  AppendResponse = Data.define(:term, :append)

  # AppendResponse#append when the log agrees with the leader's up to `index`.
  Matched = Data.define(:index)

  # What an honoured inbound message did, in two stages. `adopted` is the
  # higher term this call moved the server into (vote cleared, now a
  # follower), or nil when the term was kept. `role` is the role after the
  # call. `outcome` is what the message itself came to.
  Received = Data.define(:adopted, :role, :outcome)

  # A server's durable state, as plain data to persist however you like.
  # `entries` is an Array of Entry.
  Checkpoint = Data.define(:members, :own, :term, :voted_for, :committed, :entries)

  # Shape checks shared by the class and instance methods.
  module Shape
    module_function

    def expect(value, type)
      return value if value.is_a?(type)

      raise TypeError, "expected a #{type}"
    end

    # Entries as `term, command, term, command, ...` for the native call.
    def flatten(entries)
      expect(entries, Array).flat_map do |entry|
        expect(entry, Entry)
        [entry.term, entry.command]
      end
    end
  end
  private_constant :Shape

  class Raft
    class << self
      # The server `members[own]` as `checkpoint` left it, as a follower. The
      # membership and seat are stated, never read out of the checkpoint:
      # ArgumentError when it belongs to another membership or seat, or is not
      # a state a server can be in.
      def restored(members, own, checkpoint)
        saved = Shape.expect(checkpoint, Checkpoint)
        fields = [saved.members, saved.own, saved.term, saved.voted_for, saved.committed]
        native_restored(members, own, fields << Shape.flatten(saved.entries))
      end

      private :native_restored
    end

    def last = Position.new(*native_last)

    def entry(index)
      found = native_entry(index)
      found && Entry.new(*found)
    end

    def checkpoint
      members, own, term, voted_for, committed, log = native_checkpoint
      entries = log.each_slice(2).map { |pair| Entry.new(*pair) }
      Checkpoint.new(members.freeze, own, term, voted_for, committed, entries.freeze)
    end

    def begin_election
      request = native_begin_election
      return request if request.is_a?(Symbol)

      candidate, term, *last = request
      VoteRequest.new(candidate, term, Position.new(*last))
    end

    def request_vote(request)
      expect(request, VoteRequest)
      last = expect(request.last, Position)
      received(native_request_vote(request.candidate, request.term, [last.index, last.term])) do |term, vote, reason|
        VoteResponse.new(term, reason ? Denied.new(reason) : vote)
      end
    end

    def receive_vote(peer, response)
      vote = expect(response, VoteResponse).vote
      fields = vote.is_a?(Denied) ? [:denied, vote.reason] : [vote, nil]
      received(native_receive_vote(peer, response.term, fields))
    end

    def append_entries(request, entries)
      expect(request, AppendRequest)
      previous = expect(request.previous, Position)
      log = Shape.flatten(entries)
      answer = native_append_entries([request.leader, request.term], [previous.index, previous.term],
                                     request.committed, log)
      received(answer) do |term, append, index|
        AppendResponse.new(term, index ? Matched.new(index) : append)
      end
    end

    def acknowledge(peer, response)
      append = expect(response, AppendResponse).append
      fields = append.is_a?(Matched) ? [:matched, append.index] : [append, nil]
      received(native_acknowledge(peer, response.term, fields))
    end

    private

    def expect(value, type) = Shape.expect(value, type)

    # A refusal symbol as it is; an honoured answer as a Received whose
    # outcome the block rebuilds from the native fields.
    def received(answer)
      return answer if answer.is_a?(Symbol)

      adopted, role, outcome = answer
      Received.new(adopted, role, block_given? ? yield(*outcome) : outcome)
    end
  end
end
