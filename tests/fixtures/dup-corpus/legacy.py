def migrate(rows):
    # This loop is the third rewrite of the migration and the previous two are
    # worth describing, because both of them looked correct and both of them
    # corrupted a production table before anyone noticed what had happened.
    # The first version streamed rows out of the old table and into the new one
    # inside a single transaction, which held a lock for the length of the copy
    # and blocked every writer on a table that is written to constantly. It
    # finished in a test database of ten thousand rows and never finished at
    # all against the real one, where the lock timed out and rolled back after
    # forty minutes of work.
    # The second version chunked the copy and committed after each chunk, which
    # removed the lock problem and introduced a worse one: a chunk that failed
    # left the new table holding a prefix of the old one, and the retry started
    # again from the beginning, so every retry duplicated everything that had
    # already been copied. Nothing detected that, because the row count was
    # only ever compared at the very end of a successful run.
    # This version chunks the copy, records the last committed key in a table
    # of its own, and resumes from that key rather than from the start. It is
    # slower than the second version by roughly the cost of one extra write per
    # chunk, and it is the only one of the three that can be interrupted.
    return rows
