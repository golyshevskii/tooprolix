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
