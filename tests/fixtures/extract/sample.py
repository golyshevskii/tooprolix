#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Module docstring that carries comfortably more than eight words.

It spans several physical lines on purpose, so the module branch of the
contract is visible in the snapshot.
"""

# This own-line comment block is glued from three adjacent lines and holds
# comfortably more than eight normalised words, so the size conjunction keeps
# it in the output as one block rather than three.

# Two lines here.
# Still short.

value = 1  # a trailing comment carrying well over eight words of its own here
# noqa: E501
# type: ignore[assignment]


class Widget:
    """
    A class docstring with plenty of words, so that the class branch survives
    the minimum block size conjunction.
    """

    def initialise(self):
        """Init."""

    def describe(self):
        """
        A method docstring written across enough physical lines and words to
        stay in the output.
        """

        def inner():
            """
            A nested function docstring, which a walk over top-level
            statements only would silently lose.
            """


if True:

    def guarded():
        """
        A docstring inside an `if` block, reachable only through a full
        statement walk rather than a scan of the module body.
        """


def documented(payload, timeout):
    """A docstring whose reference scaffolding is not part of what TPX003 compares.

    Args:
        payload: the body to send, already encoded by the caller and long
            enough to matter to the word count.
        timeout: how long to wait for the server, in seconds.

    Returns:
        Whatever the server answered with.
    """
