def poll(cursor):
    # The retry budget here is deliberately small, and that matters because
    # the upstream service rate limits us on every fourth request.
    return cursor
