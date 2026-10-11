import sys

# Everything that differs between operating systems lives behind this one name.
# Both modules expose the same constants and functions.
if sys.platform == "win32":
    from . import windows as current
else:
    from . import macos as current
