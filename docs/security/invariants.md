# Where each invariant is tested

Of `threat-model.md` §6. "Not yet" means that nothing in this build could break it, or that it is missing; which one is said.

| SI | Test |
|----|------|
| 1 | `policy/caps.rs` `meet_is_the_greatest_lower_bound`, `no_descendant_has_more` |
| 2 | `policy/request.rs`, `engine/desk.rs`, `engine/tests/run.rs` `a_person_is_asked_and_their_answer_counts`, `host/tests/agents.rs` `a_person_is_asked_through_the_socket` |
| 3 | `host/tests/agents.rs` `the_secret_is_nowhere_but_in_the_vault_and_the_request`, `spawn/tests/contained.rs` `nothing_is_inherited`, `model/secret.rs` and `model/tests/transport.rs` (a canary over every error path) |
| 4 | `model/secret.rs`, `model/tests/transport.rs` (redirects, other origins) |
| 5 | `policy/rules.rs` `a_workspace_only_narrows`, `host/tests/agents.rs` `what_an_agent_may_not_be` |
| 6 | not yet: nothing of a workspace but its narrowing rules is read by the host. There is no trust step |
| 7 | `spawn/tests/only_path.rs`, `policy/request.rs` `outside_every_sandbox_is_its_own_question_and_once` |
| 8 | `spawn/tests/contained.rs` `what_cannot_be_contained_is_not_started` |
| 9 | `tools/fs.rs` `racing_a_swapper`, `a_swapped_folder_is_not_followed`; `tools/tests/tools.rs` `between_the_question_and_the_yes` |
| 10 | `tools/tests/tools.rs` `where_nothing_is_written`, `commands` (cwd) |
| 11 | `spawn/tests/macos.rs` `protected_names_cannot_be_made`, `contained.rs` `what_is_protected_is_not_written`; Linux tells afterwards (K33) |
| 12 | `policy/profile.rs`, `spawn/tests/contained.rs` `reads_no_secret_and_nothing_of_axiom`, `host/tests/agents.rs` `what_an_agent_may_not_be` |
| 13 | `spawn/tests/contained.rs` `no_network_and_no_names`. There is no egress yet: a profile with hosts is refused |
| 14 | `app/test/ui/tokens_test.dart` (no process but the host), no network code in the client |
| 15 | `app/test/transcript`, `app/test/ui/controls_test.dart` (requests whole and spelled out) |
| 16 | `policy/shell.rs` (compared with bash, zsh, sh), `policy/rules.rs` `deny_beats_everything` |
| 17, 18 | not yet: no egress, no memory, no other agents' messages |
| 19 | `policy/request.rs` `any_change_is_another_thing` |
| 20 | `host/tests` (token, peer, no IP listener) |
| 21 | not yet (K16) |
| 22 | `spawn` takes absolute programs only; the host runs no git |
| 23 | `engine/tests/run.rs` `it_works_until_there_is_nothing_to_do`, `killed_at_any_point…`; `policy/audit.rs` |
| 24 | `policy/caps.rs`, `spawn/tests/tree.rs`, `engine/tests/run.rs` `a_run_has_an_end` |
| 25 | not yet: nothing is installed |
| 26 | the audit entry's `containment`; there is one value in this build |
