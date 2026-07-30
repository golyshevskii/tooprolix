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

🔴 **Provenance, added 2026-07-30 by `exclude-reference-scaffolding-from-tpx003`. The verdicts below
are unchanged; what changed is the run they were drawn from.** The sample was selected by
`corpus/sample_clusters.py` over the `corpus/runs/*.json` written by the detector **before** that
task, i.e. at `772c1c3` / `v0.3.8`. `TPX003` now compares the narrative remainder of a block, so
`corpus/runs/` holds a different set of clusters and re-running the command in §1 no longer
reproduces these 24. Re-drawing and re-annotating a sample under the accepted marking rule is
EPIC.md Decisions #16's "protocol migration" and is deliberately **not** done here. The
post-change fate of each of these 24, measured cluster by cluster, is in that task's report.

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

### 1.4 Protocol migration — the same 24 re-judged under the accepted rule (2026-07-30)

⚠️ **§1.2 above is NOT edited and its verdicts stand as the historical record.** This section is the
**protocol migration** that EPIC.md Decisions #16 called for when it replaced §1's two-criterion
boundary rule: *"Переразметить те 24 по принятому правилу стоит, но это миграция протокола, а не
новое held-out доказательство."* Re-judging known cases is **not** new held-out evidence, and nothing
here may be reported as such.

**Why it was necessary.** Decisions #16 applied its new rule only to the six disputed clusters and
left the other 18 verdicts standing from the rule it had just superseded. Judging a
narrative-only detector against that mixture measures the new detector against the old ground truth.

**The rule applied** (Decisions #16, verbatim in substance): discard the self-contained reference
scaffolding first — templated summary line, `Args`/`Attributes`/`:param:`/returns/raises entries, and
callable-specific examples. The finding is actionable **only if** what remains asserts the same thing
in substance *and* the annotator can name a concrete canonical owner or cross-reference that removes
one copy without making either callable's reference incomplete. *"These words are similar"* is not a
basis: the annotation must name the proposed fix.

**Annotator: the supervisor. ⚠️ This re-annotation was NOT blind** — the detector's post-change
behaviour on several clusters was known beforehand. That is a bias risk and it is recorded rather
than hidden, in the same spirit as §1's provenance note. The one piece of evidence against pure
confirmation bias: **#20 was marked `no` although the detector keeps it**, i.e. a verdict was
recorded against the implementation's outcome.

| # | §1.2 said | now | why, under the accepted rule | named owner |
|---|---|---|---|---|
| 1 | yes | **yes** | pure narrative rationale in two files, no scaffolding at all | one copy cites the other |
| 2 | yes | **yes** | shared `raw_decode`/braces implementation contract beyond the summary | `v0_9` cites `models` |
| 3 | yes | **yes** | substantive shared behaviour paragraph survives the strip | `BaseCheckpointSaver.get_tuple` |
| 4 | yes | **yes** | identical paragraph on subclass runtime and the `keys` argument | "async variant of `Computer`" |
| 5 | no | **no** | templated summary only (`…with a {function\|wrap function}`) | — |
| 6 | yes | **yes** | override repeating its base; the base class is a concrete owner | `BaseAdapter.send` |
| 7 | yes | **yes** | pure narrative rationale, as #1 | one copy cites the other |
| 8 | yes | **NO — flipped** | after the strip only `…support (sync)`/`(async)` remains | none without breaking `help()` |
| 9 | yes | **yes** | shared parent-config contract paragraph survives | `BaseCheckpointSaver.put` |
| 10 | yes | **yes** | three identical substantive sentences, no `Args` at all | async cites sync |
| 11 | no | **no** | templated summary only (`a {float\|decimal} value`) | — |
| 12 | no | **no** | templated summary only (`Sends a {POST\|PUT\|PATCH} request`) | — |
| 13 | no | **no** | templated summary only (`{Update\|Delete} a webhook entry…`) | — |
| 14 | yes | **NO — flipped** | after the strip only `…(sync path)`/`(async path)` remains | none without breaking `help()` |
| 15 | yes | **yes** ⚠️ | *boundary.* Summary byte-identical across six helpers, not a per-callable template | a shared mixin/base |
| 16 | yes | **NO — flipped** | after the strip only `Initialize the {async }SQLite session` remains | none without breaking `help()` |
| 17 | yes | **yes** ⚠️ | *boundary.* Summary byte-identical; all that distinguishes them is the example | v3 cites v1 |
| 18 | yes | **yes** | three identical substantive sentences, no `Args` | `ok` owns it |
| 19 | yes | **yes** ⚠️ | *boundary.* Summary byte-identical across a service and its store | the store owns it |
| 20 | yes | **NO — flipped** | after the strip only `…with the agent{ asynchronously}` remains | none without breaking `help()` |
| 21 | yes | **yes** | shared ordering-contract paragraph survives the strip | `BaseCheckpointSaver.alist` |
| 22 | yes | **NO — flipped** | its first member is `"""` + `Args:` and nothing else — **no narrative exists** | — |
| 23 | no | **no** | templated summary only (`…with possibly multiple hosts`) | — |
| 24 | no | **no** | templated summary only (`Sends a {OPTIONS\|HEAD\|DELETE} request`) | — |

**Corrected ground truth: 13 actionable, 11 not.** Five verdicts flipped `yes → no` (#8, #14, #16,
#20, #22); none flipped the other way.

**The band, stated rather than smoothed over — same discipline as §1.3.** Three verdicts (#15, #17,
#19) are genuine boundary calls: their summary line is *byte-identical* rather than a per-callable
template, so whether the rule's "discard the templated summary line" clause reaches them is a
judgement. Marked `yes` here because a concrete canonical owner is nameable for each. Under the
strict reading they are `no`, giving **10 actionable / 14 not**. Both endpoints are reported below.

### 1.5 The narrative-only detector measured against the corrected ground truth

Detector at `feat/narrative-only-tpx003`. Of the 24 sampled clusters it now emits **15** and drops
**9** (#5, #8, #11, #12, #14, #16, #22, #23, #24).

| reading | before (all 24 emitted) | after (15 emitted) | recall |
|---|---|---|---|
| **main** (13 actionable) | 13/24 = **0.542** | 13/15 = **0.867** | 13/13 = **1.000** |
| **strict** (10 actionable) | 10/24 = **0.417** | 10/15 = **0.667** | 10/10 = **1.000** |

**Recall on this sample is 1.000 before and after: the detector drops no cluster the accepted rule
calls actionable.** The earlier reading — "four genuine findings were lost" — was an artifact of
judging the new detector against the superseded ground truth of §1.2; all four (#8, #14, #16, #22)
are `no` under the rule that replaced it.

🔴 **The residual, and the measurement that kills the obvious fix.** Exactly two false positives
survive, #13 and #20, and they are **one shape**: a templated summary line whose two copies differ by
a single token, which alone scores **0.800**. That is a narrower residual than the class this task
removed — but it is **not** closable by the threshold either, and this is measured, not assumed:

| cluster | verdict | post-change weakest |
|---|---|---|
| #13 `gitlab_webhook_store` update/delete | **not actionable** | **0.800** |
| #20 `execute_task` sync/async | **not actionable** | **0.800** |
| **#7 copied rationale, two files, zero scaffolding** | **actionable** | **0.800** |
| #2 versioned copy | actionable | 0.760 |
| #21 `list`/`alist`, four backends | actionable | 0.769 |
| #3 `get_tuple`, six backends | actionable | 0.772 |

**A genuine finding sits at exactly the same score as both survivors, and three more sit below it.**
Any threshold that removes #13 and #20 removes #7 as well, and a threshold above 0.769 removes #2,
#3 and #21. The classes still overlap after the feature fix — the same structural situation Decisions
#16 established for the pre-fix detector, on a smaller residual. **Chasing these two with the
constant is therefore ruled out by measurement, not by the Out-of-scope rule alone.**

**The same class has a 1.000 shape as well, and it is recorded so it is not rediscovered as a bug.**
Where #13 and #20 differ by one token and land at 0.800, two callables whose templated summaries are
*identical* land at **1.000** by the exact path. Constructed and measured by the supervisor: two
unrelated functions sharing `"""Sends the prepared request now.` above entirely different `Args:`
tables are reported as one finding. The floor in `is_compared` does not reach it — that floor is
[`SHINGLE_K`]-sized, and this narrative is five words. It is the templated-summary class again, not a
separate defect, and it is closable only by a rule about templated summary lines — a judgement, not a
grammar, and therefore outside what this task may decide.

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

---

## 4. Dry run — the artifact schema and the marking rule, rehearsed on calibration data (2026-07-30)

🔴 **Read this before any number below.** This is a **dry run on calibration data. It is not
held-out evidence and it may not be cited as any part of the anti-false-positive gate.** Every
cluster here comes out of `corpus/runs/*.json` — the same six repositories that calibrated the 150 /
200 word limits and the 0.75 Jaccard threshold — and the whole point of
`close-anti-fp-gate-with-public-reference` is that a number taken there describes the tuning set, not
the tool. Its purpose is narrower and is stated as such: **shake the record format and the marking
rule out before they are used where they count**, and produce numbers informed enough that the gate's
threshold is set from something rather than guessed.

⚠️ **The annotator was not blind.** The corpus numbers were fully visible before the draw, §1.4 had
already been read, and nine of the ten near clusters below turned out to be clusters §1.4 had already
judged. Same disclosure as §1.4's, for the same reason.

### 4.1 What was measured on, exactly

**Detector: `v0.4.0`.** `corpus/runs/*.json` were regenerated at `7de4c6e` (the merge of
`exclude-reference-scaffolding-from-tpx003`); the tag `v0.4.0` is `cf65fd6`; `main` is `e6adfed`.
**`git diff 7de4c6e e6adfed -- src/` is empty** — the three commits differ only in the version in
`Cargo.toml`/`Cargo.lock` — so all three carry the same detector, and the numbers below describe the
binary that goes to PyPI. AC9's tag is `v0.4.0`.

### 4.2 Population at `v0.4.0` — the carried-forward numbers, re-measured

STATE.md carried near 160 / exact 457 / total 617 forward from task 13. Re-measured from
`corpus/runs/*.json` rather than taken on trust — **confirmed exactly**:

| repo | near (`weakest < 1.0`) | exact (`== 1.0`) | total |
|---|---|---|---|
| OpenHands | 21 | 43 | 64 |
| crewAI | 51 | 43 | 94 |
| langgraph | 47 | 213 | 260 |
| openai-agents-python | 12 | 60 | 72 |
| pydantic | 27 | 94 | 121 |
| requests | 2 | 4 | 6 |
| **total** | **160** | **457** | **617** |

### 4.3 The draw

Deterministic, and it is `corpus/sample_clusters.py` that enforces it — the same sampler §1 used,
extended with `--population` and `--limit` rather than replaced:

* clusters ordered inside each repository by the finding's **own** reported address `(path, line)`;
* repositories interleaved round-robin in ASCII order of the run name;
* the **interleaved** sequence truncated to the sample size — never the individual pools, which
  would be the single-repository prefix `sample_clusters.py` exists to avoid.

```bash
CORPUS_ROOT=… uv run python3 corpus/sample_clusters.py --population exact --per-repo 20 --limit 20
CORPUS_ROOT=… uv run python3 corpus/sample_clusters.py --population near  --per-repo 10 --limit 10
```

**20 exact and 10 near, and the ratio is the point.** Exact is 457 of 617 clusters and **its
precision has never been measured**: 0.875, 0.750, 0.867 and the 0.667–0.867 band are all near-only
numbers. Whether the gate can pass at all may turn on the half nobody has looked at.

⚠️ **The near half is almost entirely a re-draw of §1.4 and carries close to no new information.**
Nine of the ten near clusters are §1.4's #1, #2, #3, #4, #7, #10, #18, #20 and #21; only near 5 is
new. That is a property of the ordering, not an accident, and it is why the near figure below should
be read as a consistency check on §1.4 rather than as evidence. The exact half is entirely new.

### 4.4 The marking rule, and the two readings it is reported under

The rule is EPIC.md Decisions #16 as restated in §1.4, applied unchanged. Stated operationally, so
that the band below is mechanical rather than a mood:

1. discard `Args` / `Attributes` / `Returns` / `Raises` / `Yields` / `:param:` / `:rtype:` entries
   and callable-specific examples and doctests;
2. discard a summary line that is a **per-callable template** — the copies differing exactly in the
   token that names their own callable, verb or version;
3. **main reading:** keep a summary line that is **byte-identical** across copies (this is how §1.4
   treated its boundary clusters #15/#17/#19). **Strict reading:** discard every summary line;
4. the finding is actionable only if what remains asserts the same thing in substance **and** a
   concrete canonical owner or cross-reference is nameable that removes one copy without leaving
   either callable's reference incomplete. Every `TP` in the artifact names that fix, and
   `corpus/classification.py` refuses to load one that does not.

**One mechanism was verified rather than assumed**, because five verdicts turn on it:
`inspect.getdoc` walks the MRO, so deleting an override's docstring leaves `help()` complete when the
base carries it. Checked in this session for both a plain method and `__init__`. That is the fact
separating *override repeats its base* (actionable) from *two unrelated callables share a summary*
(not).

### 4.5 The 30 verdicts

Full records — members, reason, named fix, FP shape, attributes — are in
`corpus/dry_run_classification.json`; the table is the index. ⚠️ marks a boundary call.

#### Exact (20)

| # | repo | first member | verdict |
|---|---|---|---|
| 1 | OpenHands | `enterprise/migrations/env.py:5-6` (+1) | **TP** |
| 2 | crewAI | `a2a/extensions/base.py:179-183` (+1) | **FP** |
| 3 | langgraph | `checkpoint/postgres/__init__.py:86-91` (+3) | **TP** |
| 4 | openai-agents-python | `examples/agent_patterns/human_in_the_loop.py:20-27` (+2) | **FP** |
| 5 | pydantic | `pydantic_core/core_schema.py:3587-3606` (+1) | **TP** ⚠️ |
| 6 | requests | `src/requests/adapters.py:137-150` (+1) | **TP** |
| 7 | OpenHands | `enterprise/migrations/env.py:86-87` (+1) | **FP** |
| 8 | crewAI | `agent/core.py:1985-1996` (+1) | **FP** |
| 9 | langgraph | `checkpoint/postgres/__init__.py:309-310` (+1) | **TP** |
| 10 | openai-agents-python | `examples/agent_patterns/human_in_the_loop.py:41-48` (+1) | **FP** |
| 11 | pydantic | `tests/serializers/test_dataclasses.py:261-266` (+2) | **TP** ⚠️ |
| 12 | requests | `src/requests/cookies.py:584-591` (+1) | **TP** ⚠️ |
| 13 | OpenHands | `enterprise/migrations/env.py:90-91` (+1) | **FP** |
| 14 | crewAI | `agent_adapters/base_agent_adapter.py:42-46` (+1) | **TP** |
| 15 | langgraph | `checkpoint/postgres/__init__.py:406-412` (+6) | **TP** ⚠️ |
| 16 | openai-agents-python | `examples/agent_patterns/human_in_the_loop.py:66-73` (+1) | **FP** |
| 17 | pydantic | `tests/serializers/test_simple.py:67-69` (+1) | **FP** |
| 18 | requests | `tests/test_requests.py:2252-2254` (+1) | **FP** |
| 19 | OpenHands | `enterprise/migrations/env.py:103-112` (+1) | **FP** ⚠️ |
| 20 | crewAI | `agent_adapters/langgraph/langgraph_tool_adapter.py:23-27` (+1) | **TP** ⚠️ |

#### Near (10)

| # | repo | first member | weakest | verdict | same cluster as |
|---|---|---|---|---|---|
| 1 | OpenHands | `enterprise/server/routes/org_profiles.py:103-104` (+1) | 0.885 | **TP** | §1.4 #1 |
| 2 | crewAI | `a2a/extensions/a2ui/models.py:258-262` (+1) | 0.760 | **TP** | §1.4 #2 |
| 3 | langgraph | `checkpoint/postgres/__init__.py:120-151` (+5) | 0.769 | **TP** | §1.4 #21 |
| 4 | openai-agents-python | `src/agents/computer.py:9-14` (+1) | 0.829 | **TP** | §1.4 #4 |
| 5 | pydantic | `pydantic_core/core_schema.py:2370-2398` (+1) | 0.780 | **FP** | — (new) |
| 6 | requests | `src/requests/models.py:838-844` (+2) | 0.889 | **TP** | §1.4 #18 |
| 7 | OpenHands | `enterprise/server/routes/org_profiles.py:255-256` (+1) | 0.800 | **TP** | §1.4 #7 |
| 8 | crewAI | `agent/core.py:766-780` (+1) | 0.800 | **FP** | §1.4 #20 |
| 9 | langgraph | `checkpoint/postgres/__init__.py:193-226` (+5) | 0.772 | **TP** | §1.4 #3 |
| 10 | openai-agents-python | `src/agents/extensions/memory/async_sqlite_session.py:22-27` (+1) | 0.971 | **TP** | §1.4 #10 |

### 4.6 The rates

Computed by `corpus/classification.py` from the records, never stored in the artifact. 95% Wilson
score intervals.

| population | FP | clusters | FP rate | 95% Wilson |
|---|---|---|---|---|
| **exact** | 10 | 20 | **0.500** | 0.299 – 0.701 |
| **near** | 2 | 10 | **0.200** | 0.057 – 0.510 |
| **combined** | 12 | 30 | **0.400** | 0.246 – 0.577 |

**Under the strict reading of step 3** — every summary line discarded, so a cluster whose only
surviving overlap is a summary line becomes not actionable — records 5, 6, 12, 14, 15 and 20 flip to
FP and no near record moves:

| population | FP | clusters | FP rate | 95% Wilson |
|---|---|---|---|---|
| exact (strict) | 16 | 20 | **0.800** | 0.584 – 0.919 |
| near (strict) | 2 | 10 | 0.200 | 0.057 – 0.510 |
| combined (strict) | 18 | 30 | **0.600** | 0.423 – 0.754 |

**The headline is the exact half, and it is the first measurement of it that exists: 0.500 under the
stated reading, 0.800 under the strict one.** Near, on a sample that is 90 % §1.4 re-judged, comes
out at 0.200 — consistent with §1.4's 13/15 precision, as it should be, and for that reason not
independent evidence of anything.

### 4.7 The false-positive shapes, and the three that are not in §1.5

Ten of the twelve FPs fall into three shapes; §1.5 named one of them.

1. **Templated / identical summary line, nothing else surviving** — §1.5's class, both its 0.800 and
   its 1.000 form. Records exact 2, 4, 8, 10, 16 and near 5, 8. §1.5 predicted the 1.000 form from a
   *constructed* pair; **it is here in the wild, five times in twenty exact clusters**, and it is the
   single largest FP shape in the sample. Near 8 is §1.5's own named residual, counted in the
   numerator exactly as Decisions #17 requires.
2. 🆕 **Third-party generated scaffolding, duplicated by a generator rather than by an author.**
   Records 7, 13, 19 — three of OpenHands' four exact clusters. Both directories hold the
   `env.py` + `script.py.mako` + `versions/` + README set `alembic init` writes; the duplicated text
   is alembic's own template. The canonical owner is upstream, outside the repository, and any
   cross-reference is overwritten the next time the file is generated. Record 19 is marked as a
   boundary because a substantive paragraph *does* survive the strip there — the rule's first clause
   passes and only the second one fails.
   ⚠️ Not in §1.5, and it is **exact-only**: a generator emits byte-identical text.
3. 🆕 **A bare external cross-reference used as a docstring.** Record 17: two adjacent tests whose
   entire docstring is `See https://github.com/pydantic/pydantic-core/pull/866`. There is no
   explanation to own — it is per-callable provenance — and removing either copy takes that test's
   link with it. Not in §1.5.
4. 🆕 **Normalisation collapses a semantically load-bearing token.** Record 18: two tests asserting
   **opposite** conditions, `with size 0` and `with size > 0`, reported as one cluster at similarity
   **1.000**. Measured with the shipped extractor rather than reasoned about — both
   `tooprolix.prose_blocks` normalisations are byte-identical, because `extract::normalize` replaces
   every non-alphanumeric character with a space and the `>` disappears:

   ```
   'ensure that a byte stream with size 0 will not set both a content length and transfer encoding header'
   'ensure that a byte stream with size 0 will not set both a content length and transfer encoding header'
   ```

   Not in §1.5, and it is a different kind of thing from the other three: the other shapes are the
   rule declining to call a real duplication actionable, this one is the detector reporting an
   identity that **does not exist in the source**.

   ➡️ **This shape was subsequently FIXED, by the owner's decision, in the same session — see §5.**
   The sentence that stood here ("recorded, not fixed") described the state before that decision and
   is superseded: `normalize` was split into a counting form and a comparison form, and this cluster
   now scores 0.750 instead of 1.000. Everything else in §4 still describes the pre-fix detector, and
   §5 is the delta.

### 4.8 One verdict changed during annotation, and it is recorded rather than smoothed

Record 12 (`requests` cookie jar helpers) was first read as an FP — identical summary line, different
parameter tables, the shape of records 2 and 8. Opening the body flipped it:
`utils.add_dict_to_cookiejar` is `return cookiejar_from_dict(cookie_dict, cj)`, a one-line wrapper,
so a canonical owner is nameable and the rule's second clause passes. Recorded in the artifact as
`flipped_during_annotation`. The direction is against the annotator's first instinct and it *lowers*
the FP rate by one, which is the direction worth disclosing.

### 4.9 The artifact and the verification mechanism

* **`corpus/dry_run_classification.json`** — one record per classified finding: run, address, member
  list, weakest similarity, near/exact, class, reason, named fix (TP) or shape (FP), attributes.
* **`corpus/classification.py`** — parses it, refuses anything malformed, and `verify()` grades it
  against the runs on disk: the SHA-256 of each run's bytes, the population re-drawn by
  `sample_clusters`, and every record's similarity, half and member list. Nothing it checks is read
  back out of a field the artifact wrote about itself.
* **`tests/unit/test_classification_artifact.py`** — 15 tests, run by `make test`, including the
  shipped artifact verifying against the shipped runs.

**Exactly two classes.** `classification` is `TP` or `FP` and the parser rejects anything else by
name, so Decisions #17's third-class loophole cannot be opened by writing `intentional` in the field;
`intentional` is permitted in `attributes`, where it does not leave the numerator. A `TP` with no
named fix and an `FP` with no named shape are both refused at load time.

**Mutation-proved, both directions:**

| mutation | result |
|---|---|
| delete one record from the committed artifact | `make test` **6 failed, 183 passed** — `test_the_dry_run_artifact_verifies_against_corpus_runs` names the lost address. Restored from a `cp` backup → 189 passed. |
| append one byte to `corpus/runs/requests.json` | `corpus/classification.py` exits **1**: `requests: … hashes d8c6dd0d…, the artifact pins 70fdf84a…`. Restored → exit 0. |

⚠️ Stated rather than glossed: the deletion reddens **six** tests, not one. Two are the guards of the
deletion itself and two are the denominator guards; the other two fail for the wrong reason, because
they build their fixtures from the shipped artifact and their fixture was the thing that was broken.
They isolate correctly while the artifact is intact, which is the state the suite runs in.

---

## 5. Re-derivation on the fixed detector (2026-07-30, same session)

🔴 **Still a dry run on calibration data, still a non-blind annotator.** §4's disclosures apply
unchanged. Re-measuring does not turn calibration data into held-out evidence.

`close-anti-fp-gate-with-public-reference` fixed the operator erasure §4.7 shape 4 measured, so §4's
numbers describe a detector that no longer exists. This section is the **delta**, not a fresh
annotation: the sample was re-drawn by the identical deterministic rule and diffed against §4's 30
records.

### 5.0 What this was measured on

⚠️ **Not a released detector.** The runs behind §5 come from a **working-tree build**, branch
`test/anti-fp-gate-holdout` on base `e6adfed`, with the operator fix applied and **not committed**.
The artifact records it as `v0.4.0 + normalize_comparable (UNCOMMITTED working tree)` rather than as
a tag, because claiming a tag the binary does not correspond to is the exact defect AC9 exists to
prevent. §4's `v0.4.0` numbers describe the released detector; these do not, and the release type
for this change is the owner's decision (see the delta in §5.2).

### 5.1 What the fix was

`extract::normalize` was serving two contracts and only one of them wanted operators erased. It is
now two functions:

| form | function | read by | operators |
|---|---|---|---|
| counting | `normalize` | `size_words` → `MIN_BLOCK_WORDS`, `TPX001`, `TPX002` | erased, as calibrated |
| comparison | `normalize_comparable` | `narrative` → `TPX003` | `<`, `>`, `=` survive as words |

**The set was chosen by measurement.** Over the 457 exact clusters, 138 have members whose raw text
differs; `>` is the sole distinguishing character in **exactly one** — record 18 itself. The
characters that fuse the most clusters are `_` (11), `.` (7) and `` ` `` (4), every one an erasure
the comparison depends on. Preserving punctuation generally would not fix a defect, it would delete
the feature.

🔴 **The first attempt applied the rule inside `normalize` itself and was wrong**, which the corpus
caught rather than review: `OpenHands` `TPX001` went **3 → 35** and `langgraph` `TPX002` **74 → 79**,
because an operator that survives is an operator `size_words` counts, silently recalibrating limits
this task may not touch. After the split, `TPX001`/`TPX002` are byte-identical on all seven rows and
`corpus/units.py --verify` still reproduces **173 volume findings exactly**.

### 5.2 Corpus delta

| | before | after |
|---|---|---|
| near | 160 | **162** |
| exact | 457 | **456** |
| total | 617 | **618** |

1 cluster appeared, **0** disappeared, 16 changed score, **0** changed membership — 17 of 617 (2.8%)
touched. The defect itself: `requests/tests/test_requests.py:2252` moved **1.000 → 0.750**.

**What that move is and is not.** It is the detector no longer claiming two opposite statements are
the *same text*: 1.000 comes off the exact path, which has no threshold and which no user,
configuration or future calibration can reach. It is **not** the finding disappearing — at 0.750 it
is still emitted, and it is still a false positive. The fix moves the class from unreachable to
reachable; suppressing it is a `SIMILARITY_THRESHOLD` decision this task may not take, and §1.5
already measured that genuine findings sit at 0.760, 0.769 and 0.772.

### 5.3 Sample delta — 29 of 30 carried forward unchanged

Re-drawn by the identical rule (`--population exact --per-repo 20 --limit 20`, `--population near
--per-repo 10 --limit 10`).

* **29 records carried forward explicitly**, each with identical score, members and population:
  exact 1–17, 19, 20 and all 10 near records. Their verdicts are re-used **because the cluster they
  describe is byte-identical**, not because re-reading them was skipped.
* **1 dropped: exact 18**, `requests/tests/test_requests.py:2252` — §4's FP record 18. It left the
  *exact* population because it is no longer exact.
* **1 new: exact 18**, `requests/tests/test_testserver.py:153-154` / `:164-165` at 1.000 — the
  cluster that moved up one slot to fill the vacancy. Annotated fresh: **TP**. Two adjacent test
  methods repeat the same two-line rationale for why their assertion matters; pure narrative, no
  scaffolding, and the enclosing class is a nameable owner. Same shape as exact 1 and 9.

**All of the movement is the fix; there is no sampling churn.** No carried-over cluster changed
score, membership or population.

### 5.4 The rates, old against new

| population | §4 (before) | §5 (after) | 95% Wilson (after) |
|---|---|---|---|
| **exact** | 0.500 (10/20) | **0.450** (9/20) | 0.258 – 0.658 |
| **near** | 0.200 (2/10) | **0.200** (2/10) | 0.057 – 0.510 |
| **combined** | 0.400 (12/30) | **0.367** (11/30) | 0.219 – 0.545 |

Strict reading: exact **0.750** (15/20, 0.531 – 0.888), near 0.200, combined **0.567** (17/30,
0.392 – 0.726).

🔴 **Read the improvement correctly, because the obvious reading of it is wrong.** Exact did not go
0.500 → 0.450 because the fix removed a false positive from the population. It went there because
the drawn sample **lost one FP and gained one TP**: the operator-collision cluster fell out of the
exact draw when it stopped being exact, and a TP moved up to fill the slot. The false positive still
exists in the corpus, at 0.750, on the near path — it is simply no longer among the drawn 30. One
sample slot changing hands is well inside the interval either way, and neither 0.500 nor 0.450 is
distinguishable from the other at n = 20.

---

## 6. Review round 1 — the operator rule became contextual (2026-07-30)

🔴 **Still calibration data, still a non-blind annotator.** §4's disclosures stand.

### 6.1 The corpus was the wrong instrument, and it said so

§5 chose the operator set `['<', '>', '=']` from a frequency count over the 138 exact clusters whose
members' raw text differs. Review found the class still open. The reason is the warning task 13
carried forward verbatim and which §5 quoted while not applying: **the corpus cannot see this class,
so a count over it can never show a set is sufficient.** Two constructed pairs, both measured at
**1.000** on the released binary:

| pair | why it collided |
|---|---|
| `value != 0` ~ `value = 0` | `!` erased, so `!=` folded to `=` |
| `limit > -1` ~ `limit > 1` | `-` erased, so the sign vanished |

§5's stated reason for excluding `!` — *"`==` yields two tokens and `!=` yields one"* — was simply
wrong, and is withdrawn rather than patched.

The mirror defect was measured too: keeping `>` unconditionally made Markdown quoting rewrite
prose. An identical paragraph, one copy prefixed `> ` per line, fell from **1.000 to 0.778** — 0.028
above the threshold, one edit from silently losing a genuine finding.

### 6.2 The rule

`is_operator_here` decides **per occurrence**, not by membership:

| character | kept when | erased when |
|---|---|---|
| `<`, `=` | always | — |
| `>` | something alphanumeric precedes it on the line | at line start — Markdown quoting, `doctest` prompt |
| `!` | immediately followed by `=` | everywhere else — sentence terminator |
| `-` | followed by a digit **and** not preceded by an alphanumeric | `non-blocking`, `utf-8` — prose hyphen |

Verified after the change: `!=`/`=` **1.000 → 0.773**, `> -1`/`> 1` **1.000 → 0.773**, and the
quoted paragraph **0.778 → 1.000**, i.e. the dilution is gone entirely.

### 6.3 The guard that can express an inequality

`a_malformed_or_unrecognised_construct_leaves_the_narrative_unchanged` asserts
`narrative(text) == normalize_comparable(text)`; both sides move together, so a constant-returning
normaliser keeps every row green. **Proved by mutation, not argued.** An equality table cannot state
"these two must stay different", so there are now two tables — `these_shapes_must_not_normalise_alike`
and `these_shapes_must_normalise_alike` — and every clause above has a row in each. Both directions
are needed: each clause can fail by keeping too much as easily as by keeping too little.

### 6.4 Corpus delta

| | §5 | §6 |
|---|---|---|
| near | 162 | **163** |
| exact | 456 | **456** |
| total | 618 | **619** |

One row moves, `pydantic` 122 → 123. `TPX001`/`TPX002` byte-identical on all seven rows and
`corpus/units.py --verify` still reproduces **173 volume findings exactly** — the counting/comparison
split did not leak. The drawn sample of 30 is **unchanged**, so §5.4's rates stand: exact **0.450**
(0.258 – 0.658), near **0.200** (0.057 – 0.510), combined **0.367** (0.219 – 0.545).

---

## 7. The held-out measurement — the gate (2026-07-30)

🟢 **This section IS held-out evidence.** Unlike §§4–6 it is not calibration data: none of the ten
repositories took part in tuning anything, and the sampling design, the threshold and the pool query
were all on `origin` before the first of them was cloned.

### 7.1 AC7 — the holdout took no part in calibration

None of the ten is one of the six `corpus.lock` repositories, and none shares an owner with one
(`openai`, `crewAIInc`, `langchain-ai`, `All-Hands-AI`, `pydantic`, `psf` are excluded by filter 1).
The 150 / 200 word limits, `SIMILARITY_THRESHOLD` = 0.75 and `SHINGLE_K` = 3 were measured on the six
corpus repositories and on nothing else (`corpus/REPORT.md`); no number in this section fed back into
any of them.

### 7.2 AC9 — what it was measured on

Detector commit **`53585f5`**, working tree **clean**, binary
`sha256 7538d48ab97e3c4fa22265440d5247e52c4508d21b5c48a398703d8a8bdaac01`, built from committed
source (`git diff 12f3e9a HEAD -- src/ Cargo.toml` is empty). The artifact pins the commit — which
`verify` requires to resolve in this repository — and the binary hash, which it checks whenever the
binary is present.

### 7.3 The blindness attestation, in both halves

**What git proves:** the pool query, the ordered pool, the draw rule, the sample size, the annotation
protocol and the threshold **0.40** were committed and pushed to `origin` before any repository was
cloned — amendments 1–3 at `6242f30`, `b7b3d29`, `297661f`/`53585f5`, each pushed ahead of the scan
it governs.

**What git cannot prove:** that nobody ran the binary against any of these ten repositories locally
beforehand. That half is an **attestation**, and it is called an attestation rather than a proof.
What is removed by construction is the discretion: which repositories (a pinned query plus mechanical
filters), which findings (the findings' own `(path, line)`), and what counts as a pass (a number
frozen before the data).

### 7.4 The ten runs

| # | repository | SHA | `.py` walked | skipped | near | exact | total |
|---|---|---|---|---|---|---|---|
| 01 | EverMind-AI/Raven | `16bdca7d` | 855 | 0 | 4 | 8 | 12 |
| 02 | GetStream/Vision-Agents | `9f2eba64` | 537 | 0 | 3 | 27 | 30 |
| 03 | Graphify-Labs/graphify | `ecfcd160` | 274 | 0 | 3 | 20 | 23 |
| 04 | HKUDS/DeepTutor | `740ec413` | 1027 | 0 | 3 | 6 | 9 |
| 05 | HKUDS/nanobot | `6a1a45d0` | 617 | 0 | 0 | 3 | 3 |
| 06 | K-Dense-AI/claude-scientific-writer | `43aaecd6` | 184 | 1 | 1 | 31 | 32 |
| 07 | Klavis-AI/klavis | `45c9f7da` | 633 | 0 | 9 | 76 | 85 |
| 08 | MemMachine/MemMachine | `a681abf9` | 475 | 0 | 6 | 24 | 30 |
| 09 | MervinPraison/PraisonAI | `688d76a1` | 4255 | 5 | 53 | 288 | 341 |
| 10 | NVIDIA/skills | `17d96f11` | 330 | 0 | 5 | 16 | 21 |
| | **total** | | | 6 | **87** | **499** | **586** |

Each run's SHA-256 is pinned in `corpus/preregistration.json` and re-checked by `verify`. Raven's run
is byte-identical to the phase-3 run that preceded amendment 2, modulo the directory rename.

### 7.5 🟢 The verdict

| population | FP | n | rate | 95% Wilson | ρ=0.10 | ρ=0.20 |
|---|---|---|---|---|---|---|
| exact | 7 | 30 | **0.233** | 0.118 – 0.409 | 0.110 – 0.427 | 0.104 – 0.444 |
| near | 4 | 10 | **0.400** | 0.168 – 0.687 | 0.167 – 0.689 | 0.167 – 0.690 |
| **combined** | **11** | **40** | **0.275** | **0.161 – 0.428** | 0.149 – 0.451 | 0.139 – 0.471 |

**`PASS: FP share 0.275 against the pre-registered threshold 0.400`, exit code 0.**

**Under the strict reading, reported as a band and not as the gate:** exact 14/30 = 0.467, near
4/10 = 0.400, combined **18/40 = 0.450**.

🔴 **State this plainly rather than bury it: the strict reading would be RED.** 0.450 > 0.400. The
gate is green because the owner fixed the **main** reading as primary — a byte-identical summary line
counts as a finding when a canonical owner can be named — and that decision was frozen on `origin`
before any of these ten repositories was cloned. Seven clusters separate the two readings (E6, E7,
E12, E16, E17, E26, E27); every one is an override or a duplicated helper whose only surviving
overlap is its summary line.

### 7.6 Per-repository rates — the clustering, shown rather than asserted

| repository | FP / n |
|---|---|
| 01-Raven | 0/5 = 0.000 |
| 02-Vision-Agents | 0/4 = 0.000 |
| 03-graphify | **3/4 = 0.750** |
| 04-DeepTutor | 0/4 = 0.000 |
| 05-nanobot | 0/3 = 0.000 |
| 06-claude-scientific-writer | 1/4 = 0.250 |
| 07-klavis | 1/4 = 0.250 |
| 08-MemMachine | 2/4 = 0.500 |
| 09-PraisonAI | 0/4 = 0.000 |
| 10-skills | **4/4 = 1.000** |

Six of ten repositories contribute **zero** false positives and two contribute **seven of the
eleven**. The consilium's correlation concern is not hypothetical: it is the dominant structure in
this result, and it is why the design effect is reported beside the nominal interval. The mean
cluster size is 4 rather than the 10 the amendment assumed, so `DEFF = 1 + 3ρ` and the combined
interval widens only to 0.139 – 0.471 at ρ = 0.20 — the conclusion does not change, but the
precision was never what the nominal figure suggested.

### 7.7 The false-positive shapes — three not previously named

Eleven false positives, and the templated-summary class that supplied 7 of the dry run's 12 accounts
for **one** of them here (N6). The holdout is dominated by shapes the calibration corpus never had.

1. 🆕 **SPDX / licence headers** — E10 (**144 members**), E20 (12), N9 (135). Mandatory per file,
   machine-read per file, and removing a copy is affirmatively wrong. Three of eleven FPs, and by
   member count they dominate the run: one finding carrying 144 addresses.
2. 🆕 **Upstream attribution notices** — E8, N7. "This is adapted from Mem0 (URL)…". Removing either
   copy strips that file of its provenance. Related to §4.7's bare-cross-reference shape but distinct:
   here the notice is substantive prose, not a bare link.
3. 🆕 **A deliberately vendored copy of the repository's own source** — E3, E13, E23, all in
   `graphify/worked/mixed-corpus/raw/`, a worked-example corpus the analyser runs on. Thinning its
   prose defeats the fixture. This single vendored file produces graphify's 3/4.
4. 🆕 **Self-contained distributable bundles** — E30, N5. A skill or plugin ships standalone, so each
   bundle must carry its own copy of a helper. Same family as the dry run's self-contained examples,
   but the constraint here is packaging rather than pedagogy.
5. **Templated summary line** (§1.5) — N6 only.

⚠️ **The single most useful thing this measurement says about the product** is not the rate. It is
that on unseen repositories the dominant false positive is **licence and attribution boilerplate**,
which the calibration corpus happened not to contain, and which no threshold can separate because it
is byte-identical by design. Naming it is inside this task; fixing it is not.

### 7.8 What this measurement does not claim

**No recall claim is made or supported.** `tooprolix` does not read `.py` files under hidden
directories — proved on a constructed fixture, and the reason `06` presents 184 of its 506 files to
the detector. That policy applied identically to the calibration corpus and does not bias
`FP / clusters emitted`, because a user running the tool sees exactly the same walk. It would matter
to a claim about findings *missed*, and this gate makes none.

### 7.9 Baseline — derived from the labels, after them

`baseline_from` takes the classified artifact and returns the addresses classified `FP`, so it cannot
be produced before the labels exist. **11 entries**, matching the numerator exactly. The order is
the one Decisions #17 requires and it is enforced by the shape of the code, not by prose: the
unsuppressed runs were saved and hashed first, every drawn finding then received exactly one class,
and only then was the baseline derived.

---

## 8. Review round 2 — the mechanism that binds the number (2026-07-30)

**The number did not move.** §7's 0.275 stands unchanged, verified against the same ten runs. What
this round repaired is the machinery that ties that number to the committed runs, plus one recall
regression this task had introduced into shipping code.

### 8.1 F1 — the pre-registered hash was pinned and never read

`preregistration.json` pinned each run's SHA-256; `RunExpectation` never loaded the field, and
`verify` compared the file on disk against `artifact.runs[].sha256` — **another field of the artifact
being graded**. Measured:

```
preregistration pin (external, authoritative): 35018b62c4a200a6
file on disk now hashes                      : 93a1f55e7f1439b8
-> PASS: FP share 0.275 ... exit 0            (FORGED RUN ACCEPTED)
```

Defect #6 at its eighth layer, and it was inside the fix that closed the seventh. The pre-registered
pin is now the authority: the file on disk is compared against it, and the artifact's own echo of it
must agree rather than substitute for it. Both halves are separately mutation-proved.

### 8.2 F2 — the child key was hardened and the container left open

Round 1 made a missing `weakest` fatal. The document around it stayed unvalidated:

```
rename top-level "findings" -> "findings_v2" in one run
-> population 586 -> 501; 07-klavis vanishes entirely, no error, no warning
```

`validate_run` now checks the document: `schema_version`, `complete`, `skipped`, `excluded` and
`findings` present and of the right type, and the top-level key set **closed** — an unknown key is
refused rather than ignored, because tolerating one is what let the rename through.

### 8.3 F4 — a recall regression this task introduced, now fixed

`<` and `=` were preserved unconditionally while `>`, `!` and `-` were contextual. A reST or Markdown
heading underline is a line of pure punctuation, so it became content tokens and **destroyed** the
match it sat beside:

| underline | before | after |
|---|---|---|
| `# =====` | **`All checks passed!`** — genuine match killed | **1.0000** |
| `# <<<<<` | `All checks passed!` | **1.0000** |
| `# -----` | 1.0000 | 1.0000 |

`-` survived only by accident — its digit clause already rejected a run of dashes — and that
asymmetry was the tell. The fix is one rule rather than five: **a line containing no alphanumeric
character anywhere carries no operators**, because it is typography. `size = 0` keeps its `=`; a
divider keeps nothing.

🔴 **The consequence was measured before the cause was accepted.** The ten holdout runs were produced
by the pre-fix binary, so a detector change would have broken AC9. Re-run of **all ten** holdout
repositories and the full corpus after the fix:

| | before | after | changed |
|---|---|---|---|
| holdout `TPX003` clusters | 586 | **586** | **0** |
| holdout run files byte-identical | — | **10 / 10** | — |
| corpus run files byte-identical | — | **7 / 7** | — |
| corpus totals | 619 (163 near / 456 exact) | unchanged | **0** |
| `units.py --verify` | 173 | **173** | — |

**Zero clusters moved anywhere, so §7's measurement stands untouched and still describes the binary
that ships.** That fact is now pinned by `TestTheHoldoutPopulationIsPinned`, which asserts the
per-repository near/exact split of all ten runs and the 586 total, so it cannot silently stop being
true.

### 8.4 F5 — the gate's own numbers were asserted by nothing

Every artifact test targeted the *dry run*. Relabelling one holdout row moved the gate from 0.275 to
0.300 while `make test` stayed green and this file went on printing 0.275. Now pinned: the 40 rows,
the 30 / 10 split, the 29 / 11 classes, the three published rates, the gate passing against 0.400,
the baseline's 11 entries, and — deliberately, because it is the uncomfortable half — the strict
reading at **18 / 40 = 0.450 and its being above the threshold**. Mutation-proved by relabelling E6:
five named tests redden.
