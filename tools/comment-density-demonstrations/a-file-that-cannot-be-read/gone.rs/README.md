# gone.rs is a directory, and that is the demonstration

The gate has to refuse a `.rs` path it cannot read, naming the path and the
reason, rather than skipping it or counting it as a file with no comments in it.
A skipped file and a file with nothing wrong in it are the same green otherwise.

A path that cannot be read has to be committed to be demonstrated on every run,
and git carries file contents rather than the reasons an open fails. A dangling
symlink is the usual shape and git can carry one, but not every checkout of this
repository restores symlinks; a directory wearing a source file's name is
carried by every checkout, is read exactly as badly, and reports its own reason
(`Is a directory`) without any setup.

The dangling-symlink shape is covered too, by
`a_file_that_cannot_be_read_is_named_and_never_skipped` in
`crates/comment-density/tests/refusals.rs`, which builds one in a scratch tree
and requires the sweep to name it.
