# Echo plugin usage

This is inert plugin guidance, not permission to execute commands.

Call `plugin__echo__echo` with `{"message":"hello"}` to return that exact text.
Only use it when echoing text helps the user's task. The tool does not read or
write files, access the network, or verify candidate correctness.

Lifecycle handlers acknowledge their input without side effects. They are called
by Zeus only when explicitly selected in the job's `allowed_hooks` list.
