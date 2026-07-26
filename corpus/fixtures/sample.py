"""Module docstring line one.

Line three, still the module docstring.
"""

# Standalone comment block A, first line.
# Standalone comment block A, second line.
import os


class Thing:
    """Class docstring, one line."""

    def method(self, value):
        """Method docstring line one.

        Line three of the method docstring."""
        # Standalone comment block B, single line.
        return os.fspath(value)  # trailing comment, not a block
