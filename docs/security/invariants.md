# Where each invariant is tested

Of `threat-model.md` §6. "Not yet" means that nothing in this build could break it, or that it is missing; which one is said.

| SI | Test |
|----|------|
| 1 | `policy/caps.rs` `meet_is_the_greatest_lower_bound`, `no_descendant_has_more`; `host/tests/peers.rs` `who_may_do_more_is_only_told_with_a_yes`, `a_delegated_agent_is_told_with_a_yes`; `host/tests/bridge.rs` (the same through a program's own tool, and what its key does not open) |
| 2 | `policy/request.rs`, `engine/desk.rs`, `engine/tests/run.rs` `a_person_is_asked_and_their_answer_counts`, `host/tests/agents.rs` `a_person_is_asked_through_the_socket` |
| 3 | `host/tests/agents.rs` `the_secret_is_nowhere_but_in_the_vault_and_the_request`, `spawn/tests/contained.rs` `nothing_is_inherited`, `model/secret.rs` and `model/tests/transport.rs` (a canary over every error path) |
| 4 | `model/secret.rs`, `model/tests/transport.rs` (redirects, other origins) |
| 5 | `policy/rules.rs` `a_workspace_only_narrows`, `host/tests/agents.rs` `what_an_agent_may_not_be` |
| 6 | `harness` (trust withheld: started only if it can be told to ignore the folder, `host/tests/delegated.rs`); of a workspace the host itself reads its narrowing rules only |
| 7 | `spawn/tests/only_path.rs`, `policy/request.rs` `outside_every_sandbox_is_its_own_question_and_once` |
| 8 | `spawn/tests/contained.rs` `what_cannot_be_contained_is_not_started` |
| 9 | `tools/fs.rs` `racing_a_swapper`, `a_swapped_folder_is_not_followed`; `tools/tests/tools.rs` `between_the_question_and_the_yes` |
| 10 | `tools/tests/tools.rs` `where_nothing_is_written`, `commands` (cwd) |
| 11 | `spawn/tests/macos.rs` `protected_names_cannot_be_made`, `contained.rs` `what_is_protected_is_not_written`; Linux tells afterwards (K33) |
| 12 | `policy/profile.rs`, `spawn/tests/contained.rs` `reads_no_secret_and_nothing_of_axiom`, `host/tests/agents.rs` `what_an_agent_may_not_be` |
| 13 | `spawn/tests/contained.rs` `no_network_and_no_names`; `tools/web.rs` (addresses, resolution at the moment of connecting, redirects), `tools/tests/tools.rs` `what_may_be_fetched`. A profile with hosts is refused: commands have no egress |
| 14 | `app/test/ui/tokens_test.dart` (no process but the host), no network code in the client |
| 15 | `app/test/transcript`, `app/test/ui/controls_test.dart` (requests whole and spelled out) |
| 16 | `policy/shell.rs` (compared with bash, zsh, sh), `policy/rules.rs` `deny_beats_everything` |
| 17, 18 | in part: what another agent says is marked as that for the model and in the transcript (`host/tests/peers.rs`); a fetched page is marked; every address is asked about. No labels that are carried on yet |
| 19 | `policy/request.rs` `any_change_is_another_thing` |
| 20 | `host/tests` (token, peer, no IP listener) |
| 21 | not yet (K16) |
| 22 | `harness/find.rs`, `host/tests/delegated.rs` `what_a_delegated_agent_may_not_be` (a program inside a workspace is not started); the host runs no git |
| 23 | `engine/tests/run.rs` `it_works_until_there_is_nothing_to_do`, `killed_at_any_point…`; `policy/audit.rs` |
| 24 | `policy/caps.rs`, `spawn/tests/tree.rs`, `host/tests/agents.rs` `a_killed_host_leaves_nothing_running`, `engine/tests/run.rs` `a_run_has_an_end`, `host/tests/peers.rs` `who_stops_an_agent_stops_what_it_began` |
| 25 | not yet: nothing is installed |
| 26 | the audit entry's `containment`; `host/tests/delegated.rs` (`none` and `harness`, and said in the question before the start) |
