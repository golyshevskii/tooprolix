def reconcile(statement, ledger):
    """Match a bank statement against the ledger and report the difference.

    This docstring is deliberately over the shipped two-hundred word docstring
    limit, because the file next to it does not parse. If the tool ever printed
    a finding for this block while the broken file failed silently, the run
    would look like a successful measurement of a repository that was never
    actually measured, which is the one outcome the exit contract exists to
    prevent. So this block is here to be withheld, not to be reported.

    Reconciliation walks the statement in posting order and folds every entry
    into a running balance keyed by the settlement date the bank assigned to
    it, rather than the date our own ledger recorded, because the bank moves
    an entry to the next business day whenever it arrives after the evening
    cut-off and our ledger does not. A naive comparison on our own dates
    therefore reports a difference on every single Friday evening batch, and
    the operations team learned years ago to ignore the report entirely, which
    is worse than having no report at all. Keying on the bank date costs one
    extra lookup per entry and removes that whole class of false alarm.

    Entries the bank has not yet settled are carried forward untouched and
    reported separately, because a pending entry is not a discrepancy and
    treating it as one would make every report noisy for two days.
    """
    return statement, ledger
