# Annotations — validate-detectors-on-reference-corpus (2026-07-26)

Every number in this file comes out of `./target/release/tooprolix` (built from `2fbf384`) via
`corpus/runs/*.json`, or out of the shipped pyo3 export `tooprolix.prose_blocks`. Nothing here was
produced by a re-implementation of a detector.

The runs the annotations are drawn from:

```bash
export PYO3_PYTHON="$(uv python find)"
cargo build --release --locked
CORPUS_ROOT=<a directory with no ancestor .gitignore> ./corpus/run_all.sh
```

`CORPUS_ROOT` must have no ancestor directory carrying a `.gitignore` — see `corpus/run_all.sh`'s
header for the one-file proof and `corpus/REPORT.md` §7.1 for the numbers.

---

## 1. AC1 — `TPX003` precision on a hand-annotated sample

### Protocol

⚠️ **Provenance, stated exactly.** The **sampling** rules — near-only, round-robin, ordering, sample
size — were fixed *before* the sample was drawn, and `corpus/sample_clusters.py` is what enforces
them. The **boundary rule** below was *not*: it was written **during** annotation, once a class of
clusters appeared that the yes/no question did not obviously decide. That is the honest order of
events and it is why §1.3 reports a band rather than a single number. A protocol written while
looking at the data is a protocol the data can bend, and the band is the size of the bend.

* **Unit of sampling: the cluster**, which is what a finding *is* since
  `change-finding-model-to-clusters`.
* **Population: near clusters only** — `weakest.similarity < 1.0`. A cluster whose weakest edge is
  exactly 1.0 holds definitionally identical text, and "should one of two identical explanations be
  merged" is not a question about a detector tuned around a 0.75 Jaccard threshold. The signal is
  conservative: a near edge scoring exactly 1.0 is counted as exact and dropped. `Cluster` carries
  no provenance field, so this is the only operational signal there is, and it is recorded rather
  than used silently.
* **Selection: round-robin, the first 4 clusters of each repository** ordered by the finding's own
  `(path, line)`. A global prefix over `(repo, path, line)` in ASCII order lies entirely inside
  `OpenHands` and never reaches `langgraph` or `pydantic`.
* **Reproduced by:** `CORPUS_ROOT=… uv run python3 corpus/sample_clusters.py --per-repo 4`.
* **Question per cluster:** *should one of the copies be deleted or merged?*
* **Boundary rule — added during annotation, not before it (see the provenance note above and
  §1.3), and STATED IN FULL below after review caught it stated in part.** The answer is **no** when
  **both** of these hold, and **yes** otherwise:

  1. the overlap between the two blocks is confined to their parameter / attribute reference entries
     (`Args:`, `Attributes:`, `:param:`), and
  2. the non-parameter content — summary line, examples, notes — says something *different* about
     each callable, so the two blocks are documenting different things and merely share a parameter
     vocabulary.

  Deleting or merging either copy then removes information about the symbol it documents. Where the
  overlap extends past the reference table — the whole block repeating bar a verb, a version token
  or `sync`/`async` — the shared text is one explanation stated twice, and the verdict is **yes**
  even when the parameter sets are identical.

  ⚠️ **Criterion 2 was missing from the earlier statement of this rule, which said only "different
  parameter sets".** Under that partial text, verdict #23 (identical parameter sets, recorded `no`)
  contradicted verdict #24 (identical parameter sets, recorded `yes`) four lines apart. All 24
  verdicts have been re-checked against the complete rule: **none changed**, because #23's two
  blocks differ in summary *and* in example while only their `Args` table coincides, and #24's
  differ in nothing but the HTTP verb. The number is therefore unchanged at 21/24.
  **The direction matters and is worth stating: the contradiction made the reported number *lower*
  than the partial rule yields** — under the partial text #23 is a mandatory `yes` and the answer
  would have been 22/24 = 0.917. The error was in the write-up, not in the annotation, and it was
  not self-serving.

### 1.1 Population

| repo | `TPX003` clusters | of them near (`weakest < 1.0`) | sampled |
|---|---|---|---|
| OpenHands | 65 | 28 | 4 |
| crewAI (`lib/crewai`) | 90 | 58 | 4 |
| langgraph | 264 | 88 | 4 |
| openai-agents-python | 70 | 13 | 4 |
| pydantic | 120 | 31 | 4 |
| requests | 8 | 6 | 4 |
| **total** | **617** | **224** | **24** |

### 1.2 The 24 verdicts

| # | repo | weakest | what the cluster is | verdict | why |
|---|---|---|---|---|---|
| 1 | OpenHands | 0.885 | the same "caller has no new key" rationale, copied between `enterprise/server/routes/org_profiles.py` and `openhands/app_server/settings/settings_router.py` | **yes** | one rationale, two files; one copy should reference the other |
| 2 | crewAI | 0.760 | `models.py` / `v0_9.py` — same A2UI extraction method on two protocol versions | **yes** | text identical bar "v0.9"; a versioned copy |
| 3 | langgraph | 0.829 | `get_tuple` on the postgres and sqlite checkpointers | **yes** | one abstract-method contract, two backends; the base class already holds it |
| 4 | openai-agents-python | 0.829 | `Computer` / `AsyncComputer` in one file | **yes** | sync/async mirror; the second could say "async variant of `Computer`" |
| 5 | pydantic | 0.805 | `plain` vs `wrap` serializer schema constructors | **no** | different parameter sets (`schema` is extra); the overlap is the shared `Args` entries |
| 6 | requests | 0.908 | `BaseAdapter.send` and `HTTPAdapter.send` | **yes** | an override repeating its own base class; docstring inheritance covers it |
| 7 | OpenHands | 0.800 | second copied rationale between the same two files as #1 | **yes** | same as #1 |
| 8 | crewAI | 0.776 | `a2a/wrapper.py` sync/async kickoff helpers | **yes** | sync/async mirror of one behaviour, adjacent in one file |
| 9 | langgraph | 0.750 | `put` on postgres, shallow-postgres and sqlite savers | **yes** | one method, three backends |
| 10 | openai-agents-python | 0.971 | `SQLiteSession` / `AsyncSQLiteSession` class docstrings | **yes** | sync/async mirror |
| 11 | pydantic | 0.759 | `float_schema` vs `decimal_schema` | **no** | different parameter sets (`max_digits`, `decimal_places`); shared numeric-constraint vocabulary only |
| 12 | requests | 0.898 | `api.post` / `api.put` / `api.patch` | **yes** | the whole block repeats bar the verb, so the overlap is not confined to the `Args` table — criterion 1 fails |
| 13 | OpenHands | 0.808 | `gitlab_webhook_store` update/delete | **yes** | the duplicated part is the project_id/group_id invariant, hoistable to the class |
| 14 | crewAI | 0.806 | `agent/core.py` sync/async error handlers | **yes** | sync/async mirror |
| 15 | langgraph | 0.882 | `_cursor` context manager on six saver/store classes | **yes** | one helper, six copies |
| 16 | openai-agents-python | 0.924 | session `__init__` sync/async pair | **yes** | sync/async mirror |
| 17 | pydantic | 0.818 | `arguments_parameter` vs `arguments_v3_parameter` | **yes** | same behaviour, two schema versions — a versioned copy, like #2 |
| 18 | requests | 0.889 | `Response.__bool__`, `__nonzero__`, `ok` | **yes** | two of the three are `return self.ok` with `ok`'s docstring copied verbatim |
| 19 | OpenHands | 0.776 | `org_service` and `org_store` paginated org listing | **yes** | one behaviour across a service and its store |
| 20 | crewAI | 0.917 | `execute_task` sync/async | **yes** | sync/async mirror |
| 21 | langgraph | 0.800 | `list`/`alist` across postgres and sqlite | **yes** | one method, four copies |
| 22 | openai-agents-python | 0.760 | four MCP server `__init__` docstrings (stdio / SSE / streamable HTTP / base) | **yes** | the overlapping part is one `cache_tools_list` explanation pasted four times |
| 23 | pydantic | 0.795 | `url_schema` vs `multi_host_url_schema` | **no** | overlap is confined to the `Args` table (identical parameter sets), while the summary and the worked example differ per constructor — criterion 1 **and** 2. The closest call in the sample; see §1.3 |
| 24 | requests | 0.793 | `Session.options` / `head` / `delete` | **yes** | same as #12: nothing but the verb distinguishes the blocks, so criterion 1 fails |

**precision = 21 / 24 = 0.875 ≥ 0.8 → AC1 met.** Re-checked verdict by verdict against the complete
boundary rule above after review found that rule stated in part; no verdict moved.

### 1.3 The sensitivity, stated rather than smoothed over

The number is stable against every judgement in the sample except one class: **API-reference
docstrings for distinct callables that share a parameter vocabulary.** Six of the 24 clusters are of
that class — #5, #11, #12, #13, #23, #24 — and they concentrate in two files
(`pydantic-core/python/pydantic_core/core_schema.py`, `requests/src/requests/{api,sessions}.py`).

Under the strictest reading — *any* pair of distinct callables each carrying its own reference table
is not actionable — all six become **no** and **precision = 18 / 24 = 0.750**, below the AC.
Under the loosest — *any* duplicated parameter documentation is mergeable — all six become **yes**
and **precision = 24 / 24 = 1.000**. Restating the boundary rule in full (§1) moved neither endpoint:
the class membership is what defines them, and it did not change.

The boundary rule at the top of §1 is the one written into this file, and it is the one closest to
the task's own wording ("deleted **or merged**"). The honest summary is therefore:

> **0.875 under the stated protocol; the whole uncertainty is one class of six clusters, and the
> band across the two extreme readings is 0.750 – 1.000.**

This is surfaced rather than averaged because it is an input to a decision, not a defect: if the
owner wants the strict reading to be the product's definition, the detector is below the AC on
API-reference prose and that is a threshold/feature question for the second epic, **not** something
this task may tune (Out of scope).

Two things the sample is *not* evidence about, by construction: exact clusters (398 of the corpus's
646 at the time of task 4a; 393 of 617 here) are excluded, and recall is not measured at all.

---

## 2. AC1b — which unit of volume is more precise

### Protocol

* **Blocks from the shipped extractor.** `tooprolix.prose_blocks(path, source)`, the pyo3 export of
  `extract()` — same function, same `>= 2 lines AND >= 8 words` filter.
* **The word count was validated against the CLI before anything was built on it.**
  `corpus/units.py --verify` replays the shipped limits (200 docstring / 150 comment, strictly
  greater) over those blocks and requires the resulting addresses to equal the `TPX001`/`TPX002`
  findings in `corpus/runs/`. Result: **173 volume findings reproduced exactly** from 11 574 blocks
  over the six runs. A mismatch aborts the run.
* **Equal alert volume.** Each alternative unit is given the threshold that fires on the same number
  of blocks, per kind, as words does. All three matched exactly:

  | unit | comment threshold | comment alerts | docstring threshold | docstring alerts |
  |---|---|---|---|---|
  | words (shipped) | 150 | 8 | 200 | 165 |
  | `chars_norm` | 810 | 8 | 1261 | 165 |
  | `tokens_norm` (cl100k) | 157 | 8 | 221 | 165 |
  | `tokens_raw` (cl100k) | 248 | 8 | 338 | **164** |

  ⚠️ **One cell is not exact, and it is printed rather than smoothed.** No `tokens_raw` threshold
  fires on exactly 165 of the 7 980 docstring blocks — the achievable counts step 164 → 166 — so the
  calibration takes the nearest reachable count within a cap of one alert, ties broken toward
  *fewer*, and the table prints what was measured rather than what was asked for. Beyond one alert
  it aborts; that is `MAX_ALERT_DRIFT` in `corpus/units.py`, and it is tested in both directions.

* **Disagreement set**, the union of the symmetric differences against words:

  | unit | fires | agrees with words | disagrees |
  |---|---|---|---|
  | `chars_norm` | 173 | 160 | 26 |
  | `tokens_norm` | 173 | 161 | 24 |
  | `tokens_raw` | 172 | 156 | 33 |

  **Union: 51 blocks.** (The AC's escape hatch — "fewer than 10 disagreements is itself the result"
  — did not apply.)

### The blindness mechanism, and what it does and does not guarantee

The annotator and the calibrator are the same agent, so blindness had to be built as data:

* `corpus/units.py --emit` writes the 58 blocks with the **proposing unit stripped**, identified
  only by a 12-character SHA-256 prefix of `(normalised text, path, line)`, **ordered by that
  digest**. The order is therefore a function of the block content alone.
* The verdicts were written into `corpus/unit_labels.tsv` (digest → yes/no) **from that file only**,
  with no access to which unit proposed which block.
* `corpus/units.py --join corpus/unit_labels.tsv` performed the join afterwards, and **the join
  fails closed**: it requires exactly one `yes`/`no` for every block of the disagreement set, no
  extras, and exits non-zero naming the offending digest. It used to be a `dict.get(..., "")`
  compared with `"yes"`, so a missing or misspelled label silently became `no` and the run still
  exited 0 — one typo quietly moving a published precision number. Proved three ways: a removed
  label, an extra label and a `probably` each abort with the digest named.
* **The population is pinned, not only the findings.** `--verify` compares finding *addresses*, so
  it constrains the ~3% of blocks that carry a volume finding and says nothing about the rest:
  review executed a walk that dropped one finding-free file (44 blocks) and `--verify` still printed
  "183 volume findings reproduced exactly". Every calibrated threshold and the whole disagreement
  set are computed over that population, so `load_blocks` now also asserts the walked `.py` count per
  run against the column `corpus/run_all.sh` records — reusing that number rather than inventing a
  second one.
* **The "alerts" column of the table below is counted from the firing set**, not echoed back from
  the request. It used to print the same variable twice, so "all three matched exactly" was a
  tautology; the property itself is enforced by `CalibrationError` and tested.

**Guaranteed:** neither the presentation order nor the identifiers carry any information about the
proposing unit; and the labels cannot be revised after seeing the join without the revision being
visible as a change to a committed file.
**Not guaranteed:** the annotator can still recognise a block *type* — a 900-word docstring looks
long whatever proposed it — so this is blindness to the unit, not to the block. And it is a single
annotator, so there is no inter-annotator agreement figure.

### The annotation question, and the trap it avoids

The question asked of each block was **"is this verbose PROSE?"**, not "is this a long block".
`Args:` / `:param:` tables, `Attributes:` tables, pasted JSON, ASCII tables, code examples and
commented-out code are length set by the *symbol*, not by the author's wordiness; annotating those
as verbose yields the rule "have fewer parameters", which is not the product. **40 of the 51 blocks
are of that kind and were labelled `no`.**

### Result

| unit | proposed (of the 51) | annotated verbose prose | precision |
|---|---|---|---|
| `tokens_norm` (cl100k) | 26 | 7 | **0.269** |
| **words** (shipped) | 26 | 6 | **0.231** |
| `chars_norm` | 26 | 6 | 0.231 |
| `tokens_raw` (cl100k) | 25 | 2 | 0.080 |

**`tokens_norm` comes out one block ahead of words, and that is reported as it fell.** Seven true
positives against six, out of 26 proposals — a margin of a single block, which this sample cannot
resolve. What the numbers do separate cleanly is the *normalised* units from the raw one:
`tokens_raw` scores 0.080, roughly a third of either, in the predicted direction — it is dragged
upward by embedded code, JSON and argument tables, where length is set by the symbol rather than by
the author.

That words and `tokens_norm` land within one block of each other is what the ratio measurement
predicted: on normalised text cl100k charges about one token per word (p50 = 1.056), so they are
very nearly the same unit measured twice. The recorded argument that separates them is not
precision but portability — cl100k charges **2.14 tokens per word on Cyrillic against 1.01 on
English**, so a token threshold fires roughly twice as early on a non-English codebase, while a word
threshold does not. **This task measures the unit and does not change it**: the unit is frozen in the
shipped JSON schema and in user-written `pyproject.toml` limits, so moving it is an owner decision
and second-epic work. Whether a one-block margin is worth reopening that is exactly the kind of call
this number exists to inform.

⚠️ **0.231 is not the volume rule's precision.** The disagreement set is by construction the set of
blocks sitting right at the calibrated boundary, where the units differ — the hardest 51 of 11 574.
The blocks all four units agree on are not in it. This number ranks the units against each other;
it is not a floor for `TPX001`/`TPX002`.

**AC1b met**: the four numbers exist and they are on a blind annotation at the shipped defaults.
**No unit change is made or proposed here** — out of scope, and an owner decision.

**Label provenance, so the reuse is auditable.** Labels are keyed by the digest of
`(normalised text, path, line)`, so a label follows its block. Of the 51 blocks in the disagreement
set, **47 already carried a label** and kept it, **4 were new** and were labelled through the same
blind mechanism — `--emit` with the proposing unit stripped, ordered by digest — and **11 labels
became unused** because their blocks are no longer in the set; those rows were dropped from
`corpus/unit_labels.tsv` rather than left to rot, and `--join` fails closed on either extras or
gaps, so a stale label set cannot silently score a run.

---

## 3. The anti-false-positive gate (issue AC4/AC5) is NOT closed by this task

Stated forward, from what the corpus is: **the gate needs a repository whose prose a human has
already cleaned by hand.** Only such a repository can tell a false positive from a real one — on any
other, a finding is just a finding, and "the tool found things" says nothing about whether it should
have. The pinned corpus contains no repository of that kind, so there is nothing here the gate can
be measured against, and it moves to the second epic together with the work of finding or building
one.

What is **not** a substitute, and is deliberately not offered as one:

* a run on a repository nobody has audited — every finding there is unlabelled, so zero, ten or a
  hundred are equally uninformative;
* an uncommittable local measurement reported as a result. Such evidence is a transcript, which
  makes it a transcript rather than an artifact — the exact defect this task closed once already for
  the AC4 marker manifest. An unclosed gate stated honestly is worth more than a closed one backed
  by nothing.

AC1, AC1b, AC2 and AC3 are unaffected: each is defined over the pinned corpus as a whole, and each
is closed on it.
