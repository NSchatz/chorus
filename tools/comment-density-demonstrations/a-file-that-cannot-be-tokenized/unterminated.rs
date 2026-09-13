pub fn fine() -> i64 {
    1
}

/* This block comment is never closed, so tokenizing runs off the end of the
   file. A counter that swallowed the error would report this file at zero
   prose, which is indistinguishable from a file with no comments in it.
   The byte offset the gate names is where this comment opened.
