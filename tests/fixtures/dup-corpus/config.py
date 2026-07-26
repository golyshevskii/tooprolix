"""Configuration loading for the settlement pipeline.

Every knob below is read once at process start and never re-read, because the
pipeline runs as a batch and a configuration change halfway through a batch
would split one run across two sets of rules. That was the behaviour before
this module existed and it produced a report nobody could reproduce, since the
answer depended on when during the run the operator had saved the file.

Precedence is environment first, then the project file, then the defaults
recorded here. Environment wins because the deployment system is the only
place that knows which cluster the process landed on, and the project file
wins over the defaults because a repository that has opinions about its own
thresholds should not have to restate them on every invocation.

Missing values are an error rather than a silent default whenever the value
changes what the pipeline reports, and a silent default whenever it only
changes how fast the pipeline gets there. The distinction is worth the extra
branch: an operator who mistypes a threshold name deserves a failure, while an
operator who omits a buffer size deserves a working process.

Unknown keys are also an error. A key the loader does not understand is either
a typo in a name that matters or a leftover from a version of the pipeline
that no longer exists, and both of those look exactly like a setting that is
quietly doing nothing at all.
"""

DEFAULT_BUFFER = 4096
