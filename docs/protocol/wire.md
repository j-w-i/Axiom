# The wire

Types: `engine/crates/proto`. Why: decision 0003. One JSON object per line, UTF-8, at most 8 MiB.

1. Connect to the Unix socket named in `<data>/run/endpoint` (`<data>/run/host.sock`, or in a short private directory if that name is too long), on every system. On Unix the host drops peers with another uid.
2. First line: `hello` with the token in `<data>/run/token` and the protocol number. Anything else, a wrong token or another protocol gets one error and the connection closes. Silence closes it after 5 s.
3. `{"id":1,"method":"session.list","params":{}}` → `{"id":1,"result":…}` or `{"id":1,"error":{"code":…,"message":…}}`. An unreadable request is answered with `id` 0 if it has none.
4. `session.follow {session, after}` is answered, then `{"session":…,"events":[…]}` messages follow: history after `after` at the client's pace, then live, in order, nothing twice. History has holes where ended items' deltas were folded.
5. `{"session":…,"resync":N}` means the client fell behind (or remembers another history, `N`=0). Nothing more comes for that session until it follows again: from its last `seq`, or from 0 after discarding what it has.
6. What was sent is in the journal. `item.ended.text` is authoritative and replaces the deltas before it.
7. Readers ignore unknown fields and keep unknown event types in sequence; an unknown enum value reads as `unknown`.
