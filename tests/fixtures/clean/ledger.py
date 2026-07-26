"""Read-only access to the ledger service."""


def fetch(order_id):
    # The ledger answers 404 for an order that has already been archived,
    # so a miss here is a normal outcome and not a transport failure.
    return _get(f"/orders/{order_id}")


def _get(path):
    return path
