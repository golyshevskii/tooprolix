def settle(batch):
    # Settlement runs after the nightly cut-off because the bank publishes
    # its exchange rates once per day and earlier totals would be revised.
    return sum(batch)
