# Pre-registration — the anti-false-positive gate

**Status: DRAFT. Not yet the pre-registration.** Two values below are decisions the owner has to
make, and this file becomes the pre-registration only once they are filled in, committed and
**pushed to `origin`**. AC2's proof is the commit order on the remote, not a sentence in a report:
this file must land on `origin` **before `tooprolix check` is run against a single candidate
repository**.

Task: `close-anti-fp-gate-with-public-reference`. Rule of record: EPIC.md Decisions #16 (what counts
as a finding) and #17 (what the gate measures).

---

## 0. Open decisions — this file is not pre-registered until these are closed

### 0.1 ✅ CLOSED — the threshold is 0.40, judged on the main reading

> **THRESHOLD = `0.40`**

**Decided by the owner, 2026-07-30, before the detector had seen the holdout.** The commit that
carries this number is on `origin` ahead of any clone or scan of a pool repository, and that order —
not this sentence — is the evidence.

The gate is **green** iff the primary rate of §5 is **≤ 0.40**, and **red** otherwise. One number,
one comparison, no second criterion.

**The primary rate is the FP share under the MAIN reading** — boundary clusters (a summary line that
is byte-identical rather than a per-callable template) count as findings when a canonical owner can
be named. The **strict** reading, which counts every such cluster as a false positive, is reported
beside it as a band and is **not** the number compared to the threshold. This matches how
`annotations.md` §1.4 and §1.5 already report, so the holdout figure is comparable with them.

**Why 0.40 and not a rounder, safer number.** The calibration dry run on the fixed detector measured
0.367 combined under the main reading. 0.40 sits just above it, so the outcome is not predetermined
in either direction: at n=40 the holdout's own interval is about ±0.15 and it can genuinely land on
either side. A threshold of 0.60 would have passed under both readings' point estimates, which is
not a gate; a threshold of 0.30 would have pre-registered a near-certain failure, which is not a
test. **A roughly even chance of red is the property being bought here, and it is deliberate.**

If it comes back red, §7's stopping rule applies without amendment: report the numbers and stop.
Publication waits for a task that closes the templated-summary class — which supplied 7 of the 12
false positives the dry run saw — and not for a revised threshold.

Everything the dry runs of `corpus/annotations.md` §4 and §5 measured is input to this choice and is stated
there with their intervals — including the fact that the exact half of the population had never been
measured before and came out, on the fixed detector, at 0.450 (0.258 – 0.658) under the stated reading and 0.750
(0.531 – 0.888) under the strict one.

### 0.2 ✅ CLOSED — the labelled set is the pre-registered sample of 40

**Decided by the owner, 2026-07-30.** AC5's "разметка **каждой** сырой находки" and ToDo 3's fixed
sample size of 40 could not both hold on a repository that emits hundreds of clusters. It is
resolved in favour of the sample: **the 40 drawn findings each carry exactly one class; the emitted
population is declared but not individually labelled.**

**Why this does not reopen the Decisions #17 loophole.** That loophole is a *selection* effect — a
baseline taken before annotation decides what gets annotated, so the findings themselves choose
whether they are counted. Here nothing about a finding influences whether it is drawn: the 40 are
selected by §5.2's rule, which is fixed in this file **before the run exists**, and which reads only
the findings' own addresses `(path, line)` and their near/exact half. An annotator who dislikes a
finding cannot keep it out of the sample, and one who likes it cannot pull it in.

**What the artifact must therefore carry**, and `corpus/classification.py` enforces the shape:

* the **full emitted population** — every run's SHA-256 and its `TPX003` cluster count, near and
  exact — so the denominator the sample estimates is on the record;
* the **draw declaration** (`population`, `per_repo`, `limit`), from which `verify()` re-derives
  exactly which 40 were drawn and fails if the artifact's rows are not that set;
* one class per drawn finding, `TP` or `FP`, and nothing in between.

The unlabelled remainder is therefore **visible and countable** rather than silently absent: a
reader can see that N clusters were emitted, 40 were labelled, and precisely which 40. Adding a
third class for the remainder remains **not** an option — Decisions #17 closed that by name.

---

## 1. The population, and why none of the corpus repositories can be in it

**Target population:** public open-source Python agent / LLM-framework repositories — the same
population `corpus/corpus.lock` describes as what the tool is aimed at ("OSS agent frameworks — the
population the tool is aimed at: prose largely written and rewritten by coding agents").

**AC7 — the six pinned corpus repositories are excluded, by name, because they calibrated the
detector.** The 150 / 200 word limits and the 0.75 Jaccard threshold were measured on these and only
these (`corpus/REPORT.md`), so a false-positive share taken on any of them describes the tuning set:

| excluded | reason |
|---|---|
| `openai/openai-agents-python` | in `corpus.lock`; calibrated the thresholds |
| `crewAIInc/crewAI` | in `corpus.lock`; calibrated the thresholds |
| `langchain-ai/langgraph` | in `corpus.lock`; calibrated the thresholds |
| `All-Hands-AI/OpenHands` | in `corpus.lock`; calibrated the thresholds |
| `pydantic/pydantic` | in `corpus.lock`; calibrated the thresholds |
| `psf/requests` | in `corpus.lock`; calibrated the thresholds |

**Their owners are excluded too**, and mechanically: any repository whose GitHub owner is one of
`openai`, `crewAIInc`, `langchain-ai`, `All-Hands-AI`, `pydantic`, `psf`. Documentation style is an
organisational habit — `langchain-ai/langchain` shares conventions, tooling and often authors with
`langgraph` — so a same-owner repository is not held out in the sense that matters.

The holdout has never been run through `tooprolix`, has never influenced a constant, and appears in
no measurement in `corpus/REPORT.md` or `corpus/annotations.md`.

---

## 2. Inclusion filters — objective, and applied mechanically

Applied in this order to the enumerated pool of §3. Each is a property of the repository that a
stranger can re-check without judgement; none of them mentions prose, style or "signs of a
hand-cleaned repository", because those are exactly the subjective criteria a person who already
knows where `TPX003` is weak would apply.

| # | filter | how it is checked | fails closed |
|---|---|---|---|
| 1 | not one of the six `corpus.lock` repositories, and not owned by one of their owners | string comparison against §1 | — |
| 2 | public, not a fork, not archived | `gh api repos/<owner>/<name>` → `.fork == false`, `.archived == false` | a repository the API cannot describe is excluded |
| 3 | primary language Python | `.language == "Python"` | `null` excludes |
| 4 | permissive OSI licence: SPDX id in `{MIT, Apache-2.0, BSD-3-Clause}` | `.license.spdx_id` | `NOASSERTION` / `null` excludes — an unidentified licence is not a permissive one |
| 5 | actively maintained: `pushed_at` within 90 days of 2026-07-30 | `.pushed_at` | missing excludes |
| 6 | at least **250** tracked `.py` files at the resolved SHA | `gh api repos/<o>/<n>/git/trees/<sha>?recursive=1`, counting blobs ending `.py`; `truncated == true` aborts | a tree that cannot be counted excludes |

**Where 250 comes from — measured, not chosen.** Over the six corpus repositories the density of
`TPX003` clusters per tracked `.py` file ranges from 0.076 (`openai-agents-python`: 72 / 840) to
0.582 (`langgraph`: 260 / 447), with `requests` at 0.162 (6 / 37). At that middle density a
repository needs ≈ 247 `.py` files to emit the 40 clusters this measurement samples. 250 is that
number rounded, and it is a floor on the *chance* of filling the sample from one repository, not a
guarantee — §5's walk covers the shortfall.

**Resolution of a SHA is exact-match, and this is a real trap.** `git ls-remote <url>
refs/heads/<branch>` matches on a suffix: for `letta-ai/letta` it returns **two** lines,
`refs/heads/main` and `refs/original/refs/heads/main`. The rule is: take the line whose ref is
**exactly** `refs/heads/<default_branch>`; zero or more than one exact match aborts the resolution
rather than picking one.

---

## 3. The candidate pool — enumerated, resolved, and frozen

Resolved 2026-07-30 with `gh api` and `git ls-remote`. **No repository below has been cloned and
none has been run through `tooprolix`.** Changing any line of this table after this file is pushed
invalidates the measurement.

### 🔴 3.0 AMENDED 2026-07-30 (review round 1, finding B6) — §3.1/§3.2 below are SUPERSEDED

**This file was already pushed when this amendment was written, and the amendment is append-only on
purpose.** AC2's proof is the ordering of commits on `origin`, so nothing above is rewritten and the
superseded tables stay visible. It is legitimate to change the pool at all only because **no
candidate repository has been cloned, fetched or scanned** — the moment one is, the pool is frozen
for good.

**What was wrong.** §3.1 was a table I assembled by hand from twelve repositories I thought of, and
§3.2 disclosed that as residual discretion. Disclosure is not removal: the ordering rule
auto-selects entry 1, so whoever chooses the membership chooses the holdout. The table below is
replaced by a **query**, so a stranger reproduces the pool instead of trusting the list.

**The pinned query**, run against the GitHub REST search API on **2026-07-30**:

```bash
gh api -X GET search/repositories \
  --raw-field 'q=language:python topic:ai-agents stars:>2000 pushed:>2026-05-01' \
  --field sort=stars --field order=desc --field per_page=100
```

`topic:ai-agents` is the one discretionary choice left, it is disclosed here, and it was taken
because it returns the population `corpus/corpus.lock` names — `crewAI`, `langgraph`, `agno`, `mem0`,
`browser-use` all come back under it. `topic:llm` was rejected as measured: it returns 269
repositories including inference engines and tutorial collections. **`sort` cannot influence
membership here**: `total_count` is **92** against `per_page=100`, so every match is returned and
the ordering of the response is discarded.

The mechanical filters of §2 are then applied to whatever the query returns, and the pool is ordered
by **ASCII ascending on the clone URL** exactly as before.

| stage | repositories |
|---|---|
| returned by the query | 92 |
| after filters 1–4 (owner, fork/archived, language, licence) | 79 |
| after filters 5–6 (ref resolves to exactly one, ≥ 250 `.py`) | **45** |

**⇒ The holdout is entry 1, `EverMind-AI/Raven` at `16bdca7d1989e38b1b8bf18fa4e5586991e6817f`**
(Apache-2.0, 855 `.py`). **It is not `agno-agi/agno`**, which the superseded hand-built table
selected; `agno` is now entry 18. That change is the honest outcome of removing the discretion and
it is taken as it fell.

⚠️ **Named honestly: the query admits repositories that are not agent *frameworks*** — skills
collections and tutorial repositories reach the `.py` floor (`agentic-awesome-skills`,
`Anthropic-Cybersecurity-Skills`, `ai-engineering-from-scratch`). A seventh filter was measured as a
candidate fix — *declares an installable package at the repository root* — and **rejected on the
measurement**: it drops 13 of the 45, and among them are genuine frameworks `agno` (4295 `.py`),
`PraisonAI`, `airweave` and `E2B`, because it selects on **monorepo layout rather than on kind**.
It also changes nothing that matters: `EverMind-AI/Raven` is entry 1 under both filter sets. A filter
that excludes real members of the population and moves no outcome is not worth its discretion.

### 3.0.1 The pool, as the query returned it

| order | clone URL | resolved SHA | licence | `.py` |
|---|---|---|---|---|
| 1 | `https://github.com/EverMind-AI/Raven.git` | `16bdca7d1989e38b1b8bf18fa4e5586991e6817f` | Apache-2.0 | 855 |
| 2 | `https://github.com/GetStream/Vision-Agents.git` | `9f2eba647f40c5c3efe20943a2018d8b44bd0d6c` | Apache-2.0 | 537 |
| 3 | `https://github.com/Graphify-Labs/graphify.git` | `ecfcd160d56b420eb8241430fa7b5b1951c7829f` | Apache-2.0 | 274 |
| 4 | `https://github.com/HKUDS/DeepTutor.git` | `740ec413a0ce56145ef02d63e181715d207b8b11` | Apache-2.0 | 1027 |
| 5 | `https://github.com/HKUDS/nanobot.git` | `6a1a45d07a6de420ba87c419ae30fcb4af76d4d0` | MIT | 617 |
| 6 | `https://github.com/K-Dense-AI/claude-scientific-writer.git` | `43aaecd6a24bb949b5c5c5b7e7105963e1abd53e` | MIT | 506 |
| 7 | `https://github.com/Klavis-AI/klavis.git` | `45c9f7da83d1cf43f7429b96f9c8e8153542ea1e` | Apache-2.0 | 636 |
| 8 | `https://github.com/MemMachine/MemMachine.git` | `a681abf9623299bba8ad931e5d9af02fb6ef0997` | Apache-2.0 | 475 |
| 9 | `https://github.com/MervinPraison/PraisonAI.git` | `688d76a18f344e91077e8c94314fa44d699fe2ab` | MIT | 4270 |
| 10 | `https://github.com/NVIDIA/skills.git` | `17d96f116344cf662a658370a31582d3633d95c8` | Apache-2.0 | 335 |
| 11 | `https://github.com/NousResearch/hermes-agent.git` | `14abd64b00bbd5d0f2d6207d21ce50e2c36141c8` | MIT | 3658 |
| 12 | `https://github.com/Ontos-AI/knowhere.git` | `e0bbc899ab77168154d05184e4e835e2a6069393` | Apache-2.0 | 624 |
| 13 | `https://github.com/Project-N-E-K-O/N.E.K.O.git` | `e4501bdf6aa782b0b1fba737fd6ecb4df558e93b` | Apache-2.0 | 2543 |
| 14 | `https://github.com/SolaceLabs/solace-agent-mesh.git` | `00d3417e4a299cb40cc528bf4dc48fce833d4649` | Apache-2.0 | 974 |
| 15 | `https://github.com/ag-ui-protocol/ag-ui.git` | `bb1c2afddb4880309879b9564cfb3a635a5da4eb` | MIT | 441 |
| 16 | `https://github.com/agentscope-ai/ReMe.git` | `f3d32e203d846be1244cde3a12638b2aba74c5ac` | Apache-2.0 | 272 |
| 17 | `https://github.com/agentuniverse-ai/agentUniverse.git` | `254ecd280f54c2d2de654bbe5373e9f947d2dedf` | Apache-2.0 | 1758 |
| 18 | `https://github.com/agno-agi/agno.git` | `7c68873c1357321a5152397c8ab4fb8b3f587bba` | Apache-2.0 | 4295 |
| 19 | `https://github.com/airweave-ai/airweave.git` | `1ebe1af2dbfb90f3334410721e69997e4f02b320` | MIT | 1523 |
| 20 | `https://github.com/browser-use/browser-use.git` | `f0aa3a8bb03779c71a5aa262d389e3bfe6b77cdc` | MIT | 370 |
| 21 | `https://github.com/bytedance/deer-flow.git` | `150f7740c703292a154762f3a71aa9e18d17bda3` | MIT | 1078 |
| 22 | `https://github.com/datachain-ai/datachain.git` | `1955f8e5b31b3bd49ba57e454c96af3336ee6339` | Apache-2.0 | 438 |
| 23 | `https://github.com/e2b-dev/E2B.git` | `4a2571d321879b4d29b9d8a73bcea56935bd6cdb` | Apache-2.0 | 446 |
| 24 | `https://github.com/emcie-co/parlant.git` | `ea737442b8ae65854a842542e544fbe7e6144bad` | Apache-2.0 | 298 |
| 25 | `https://github.com/foryourhealth111-pixel/Vibe-Skills.git` | `f627ab556f8761c4d29874a896b0cfdb71278478` | Apache-2.0 | 676 |
| 26 | `https://github.com/google/adk-python.git` | `fa31b6ca9886eb48b9ac9c0dfe4f70c4443e1488` | Apache-2.0 | 1702 |
| 27 | `https://github.com/gptme/gptme.git` | `2b7d7720166873e223573af7e59ee5a6edc5b2f1` | MIT | 785 |
| 28 | `https://github.com/jeremylongshore/claude-code-plugins-plus-skills.git` | `b3855fa42128c0b2313374e5cf30d34144d202ca` | MIT | 947 |
| 29 | `https://github.com/julep-ai/julep.git` | `fc74d079a18c8124b2627ca4717f5a9c269267db` | Apache-2.0 | 478 |
| 30 | `https://github.com/letta-ai/letta.git` | `b76da9092518cbaa2d09042e52fdcbde69243e18` | Apache-2.0 | 878 |
| 31 | `https://github.com/mem0ai/mem0.git` | `74f6dc6f0d60906c4babf762fc8d14b7169c196c` | Apache-2.0 | 362 |
| 32 | `https://github.com/microsoft/agent-governance-toolkit.git` | `c38d90ae5ae11ad9635cee876763dcb49dd0f3e4` | MIT | 1879 |
| 33 | `https://github.com/microsoft/apm.git` | `634f7b603a8c827ab5c2a7c776ba2e470b1303eb` | MIT | 1596 |
| 34 | `https://github.com/mukul975/Anthropic-Cybersecurity-Skills.git` | `673da1f3b0b7be34ffc9624ef3858fe45f1c3bed` | Apache-2.0 | 1094 |
| 35 | `https://github.com/nesquena/hermes-webui.git` | `0a401597594575d5650a755d1228b7de5a87544e` | MIT | 1390 |
| 36 | `https://github.com/neuml/txtai.git` | `8362110c6428305c4b65a3283731a1239cfd1632` | Apache-2.0 | 378 |
| 37 | `https://github.com/omnigent-ai/omnigent.git` | `249f1eb6a8d2b96b44559b813a0c88e073beaced` | Apache-2.0 | 2115 |
| 38 | `https://github.com/opensquilla/opensquilla.git` | `f662be398b6b0b44034906070735cbba1651a2ba` | Apache-2.0 | 2275 |
| 39 | `https://github.com/potpie-ai/potpie.git` | `b5a677429481e0c93faa9841a9d9ce02ced95e35` | Apache-2.0 | 773 |
| 40 | `https://github.com/rohitg00/ai-engineering-from-scratch.git` | `7157ca74a135fad2165f680ec4b4e592f075ec21` | MIT | 604 |
| 41 | `https://github.com/sickn33/agentic-awesome-skills.git` | `ad3549b200584deb0c21eb7a05bc93d4fdc3714d` | MIT | 2120 |
| 42 | `https://github.com/strands-agents/harness-sdk.git` | `4433e9a394f2c4a0c51c6cacfec0f47bc978df94` | Apache-2.0 | 669 |
| 43 | `https://github.com/topoteretes/cognee.git` | `88aa09b4e3289e3dbf12c0c090080920816e2fb7` | Apache-2.0 | 1897 |
| 44 | `https://github.com/truera/trulens.git` | `10a071b9a03a6130784b861fe15aff0531cc87c9` | MIT | 504 |
| 45 | `https://github.com/zhayujie/CowAgent.git` | `9ef066c0029b2be4536ce09e39fa4a7e8d24a5ce` | MIT | 344 |

**Rejected at filters 5–6: 34 repositories**, every one on the `.py` floor (the largest near miss is
`yaojingang/yao-meta-skill` at 235, the smallest `KhazP/vibe-coding-prompt-template` at 1). No
repository was rejected for an unresolvable ref. The four rejects of the superseded §3.2 are kept
below as the historical record.

---

### 3.1 SUPERSEDED — the hand-assembled pool

Ordered by **ASCII ascending on the clone URL string**, which is a pure function of the names and
carries no information about the detector. Stars, size and recency are deliberately *not* the key:
they move, and a person who knows the pool can guess where they land.

| order | clone URL | resolved SHA | licence | `.py` |
|---|---|---|---|---|
| 1 | `https://github.com/agno-agi/agno.git` | `7c68873c1357321a5152397c8ab4fb8b3f587bba` | Apache-2.0 | 4295 |
| 2 | `https://github.com/browser-use/browser-use.git` | `f0aa3a8bb03779c71a5aa262d389e3bfe6b77cdc` | MIT | 370 |
| 3 | `https://github.com/camel-ai/camel.git` | `ec48f997f3c2a700ae5a4cf0280792838fea81f8` | Apache-2.0 | 1140 |
| 4 | `https://github.com/deepset-ai/haystack.git` | `88e8b8ae857765606560745d950b6569f04c01e8` | Apache-2.0 | 529 |
| 5 | `https://github.com/letta-ai/letta.git` | `b76da9092518cbaa2d09042e52fdcbde69243e18` | Apache-2.0 | 878 |
| 6 | `https://github.com/mem0ai/mem0.git` | `74f6dc6f0d60906c4babf762fc8d14b7169c196c` | Apache-2.0 | 362 |
| 7 | `https://github.com/run-llama/llama_index.git` | `c864fcfa2c1d1f987ccdbcdab7b18e395c01ba86` | MIT | 3837 |
| 8 | `https://github.com/stanfordnlp/dspy.git` | `0312f0da6005ed5b30853d79c5c2bc91ea765a84` | MIT | 261 |

**⇒ The holdout is entry 1, `agno-agi/agno` at `7c68873c1357321a5152397c8ab4fb8b3f587bba`.** Entries
2 – 8 are walked in order only if the sample cannot be filled (§5).

### 3.2 SUPERSEDED — rejects of the hand-assembled pool

| candidate | filter | value |
|---|---|---|
| `Aider-AI/aider` | 6, `.py` floor | 147 `.py` (SHA `5dc9490bb35f9729ef2c95d00a19ccd30c26339c`, Apache-2.0) |
| `huggingface/smolagents` | 6, `.py` floor | 75 `.py` (SHA `e3a5b8994b301983b91c0325546e9dc82eab8cf0`, Apache-2.0) |
| `BerriAI/litellm` | 4, licence | `spdx_id` = `NOASSERTION` |
| `microsoft/autogen` | 4, licence | `spdx_id` = `CC-BY-4.0` — not a permissive software licence |

⚠️ **The discretion that remains, named.** The order and the filters remove all discretion about
*which* member of the pool becomes the holdout. They do not remove the choice of pool *membership*,
which was made by the annotator from knowledge of the ecosystem. That is why the whole pool is
enumerated here — including the four rejects and the numbers that rejected them — rather than
summarised as "criteria".

---

## 4. The annotation protocol — verbatim from EPIC.md Decisions #16

> Для докстрингов разных callable сначала отбрасывается самостоятельный справочный каркас: шаблонная
> summary-строка, записи `Args`/`Attributes`/`:param:`/returns/raises и callable-специфичные примеры.
> Находка считается **actionable только если** оставшееся пересечение утверждает одно и то же по
> существу — поведение, обоснование, оговорку или контракт реализации — **и** аннотатор может назвать
> конкретного канонического владельца или кросс-ссылку, которая убирает одну копию, не делая справку
> ни одного из callable неполной. Иначе — не находка. **«Эти слова похожи» основанием не является:
> разметка обязана назвать предлагаемое исправление.**

Operationally, as `corpus/annotations.md` §4.4 states it and `corpus/classification.py` enforces it:

1. discard `Args` / `Attributes` / `Returns` / `Raises` / `Yields` / `:param:` / `:rtype:` entries
   and callable-specific examples and doctests;
2. discard a summary line that is a **per-callable template**;
3. keep a summary line that is byte-identical across copies (the main reading; the strict reading
   discards it, and both are reported);
4. actionable only if what remains asserts the same thing in substance **and** a concrete canonical
   owner or cross-reference is nameable that removes one copy without leaving either callable's
   reference incomplete.

**Exactly two classes, TP and FP** (Decisions #17). A finding that is rejected while its prose stays
in place is an **FP**. It may carry `intentional` in `attributes`; it may not become a third class,
because a third class removes it from the numerator and turns any failed gate into a pass by
renaming. `corpus/classification.py` refuses any other value in the `classification` field, refuses a
TP that names no fix, and refuses an FP that names no shape.

**The known residual is counted in the numerator, not excused.** The templated-summary class
(`corpus/annotations.md` §1.5) is not closable by any threshold — a genuine finding sits at exactly
0.800 with it, measured — and it appeared five times in twenty exact clusters in the dry run. It will
appear in the holdout. It is an FP there, and if it fails the gate that is a red gate and a report,
not a reason to redefine the class.

---

## 5. The measurement — one denominator, one sample, one stopping rule

### 5.1 The primary numeric denominator

> **`FP / TPX003 clusters emitted`**, evaluated over the pre-registered sample of §5.2.

Not "and/or `FP / blocks`". Not the volume rules — `TPX001` and `TPX002` findings are not in the
numerator or the denominator, and are not annotated.

**If a repository emits zero `TPX003` clusters:** it contributes zero to both numerator and
denominator, the fact is recorded in the artifact with its run hash, and the walk continues to the
next repository in the §3.1 order. A repository that emits nothing can neither pass nor fail the
gate — a rate over an empty denominator is reported as `unavailable`, never as `0.0`.
`corpus/classification.py` returns `None` rather than `0.0` for exactly this reason.

### 5.2 Sample size 40, allocated proportionally so the combined figure is a plain ratio

Let `E` and `N` be the exact (`weakest.similarity == 1.0`) and near (`< 1.0`) cluster counts of the
first repository's run, `T = E + N`.

* `n_exact = int(40 * E / T + 0.5)`, clamped into `[5, 35]`; `n_near = 40 - n_exact`.
  Explicit `+ 0.5` rather than `round()`, whose banker's rounding would make the allocation depend on
  parity.
* Draw the **first `n_exact` exact clusters** and the **first `n_near` near clusters**, each ordered
  by the finding's own reported address `(path, line)`, by `corpus/sample_clusters.py`
  `--population exact|near --limit <n>`.

Proportional rather than equal allocation is what makes the **combined** number a plain `FP / 40`
with an ordinary Wilson interval instead of a weighted estimate; the two halves are still reported
separately, as AC8 requires. The clamp guarantees neither half is empty when the population has
enough of it.

**Reported: three numbers, each with a 95% Wilson score interval** — exact, near, combined. The
**combined point estimate is the gate**; the intervals are reported for honesty and are *not* a
second criterion.

### 5.3 The stopping rule

* **More than 40 available in the first repository** — the truncation above *is* the rule: the first
  `n_exact` and first `n_near` by `(path, line)`. Nothing is chosen.
* **Fewer than 40** — walk to entry 2 of §3.1, then 3, and so on, filling **only the half that is
  short**, keeping each repository's own `(path, line)` order and appending repositories in the §3.1
  order. Stop at the first repository that completes the sample.
* **The whole pool exhausted with a half still short** — report that half's rate on the clusters that
  exist, state `n` explicitly next to it, and never pad it from the other half. If the combined `n`
  is below 40 the gate is still evaluated, with `n` named in the result.

⚠️ **Every repository the walk touches is burned.** Once `tooprolix check` has run against entry 2,
entry 2 is no longer held out and may not be re-used as a fresh holdout later.

### 5.4 If the threshold is missed

**The task reports a red gate to the owner, with the numbers.** That is a valid, complete outcome of
this task.

Explicitly **not** permitted as a response to a red gate:

* retuning any detector constant — the 150 / 200 word limits, `SIMILARITY_THRESHOLD`, `SHINGLE_K`.
  The holdout exists to test the constants; tuning them against it is the over-fitting this
  measurement was designed to prevent, and it is in the task's Out of scope;
* re-annotating the sample under a rule invented after seeing the result;
* re-picking the holdout, or walking further into the pool to find a friendlier repository;
* suppressing findings into a baseline or with markers to reach `exit 0`. `exit 0` reached by
  suppression closes no AC.

---

## 6. Blindness — an attestation, and it is called that

**Git proves when a file was committed; it cannot prove that nobody ran the binary locally.** The
ordering of commits on `origin` is evidence that this file existed before the holdout run was
committed. It is not evidence that the holdout was never run before this file was written. That
residue is an attestation by the person who ran the measurement, and it is recorded as an attestation
rather than presented as a proof.

What is actually removed by construction, and what is not:

* **removed:** the choice of *which* repository is measured — §2 and §3.1 make it a function of
  public metadata and a string sort;
* **removed:** the choice of *which* findings are annotated — §5.2 makes it a function of the
  findings' own addresses;
* **not removed:** the choice of pool membership (§3.2 discloses it in full), and the honesty of the
  verdicts. The named proposed fix in every TP is what makes a verdict re-checkable by a reader, and
  `corpus/classification.py` refuses a TP without one.

The dry runs of `corpus/annotations.md` §4 and §5 were **not** blind and say so; they are calibration data and
no part of the gate.

---

## 7. Baseline ordering — a procedure, in this order, checked by artifacts

Decisions #17: *the baseline is an output, not an input.* A baseline taken before annotation is a
filter that decides what gets annotated.

1. **Run and save.** `tooprolix check --format json` over the pinned checkout, with **no baseline and
   no suppression markers of any kind**. The checkout root must be **outside**
   `/Users/vgolyshevskii/dwh` — a parent `.gitignore` makes the walker read roughly one file per
   repository while still printing `complete: true`. The walked `.py` count is recorded next to the
   run and must be within a stated tolerance of the `.py` count of §3.1.
2. **Hash.** SHA-256 of the saved JSON's bytes, recorded in the classification artifact. This is what
   ties every later claim to a specific set of findings.
3. **Label.** Every finding in the pre-registered sample gets exactly one class, `TP` or `FP`, with
   a named fix or a named shape, into the artifact — *before* anything is suppressed.
4. **Only then, the baseline**, derived from the labelled artifact.
5. **Verify.** `corpus/classification.py` re-reads the run, re-hashes it, re-draws the sample and
   checks every record's similarity, half and member list against the run's own bytes. Nothing it
   checks is read back out of a field the artifact wrote about itself. It is the `--verify`
   *analogue* AC5 permits; `tooprolix` has no `--verify` flag and no baseline feature, and adding one
   would be a linter feature rather than a measurement.

### 7.1 AMENDED 2026-07-30 (review round 1) — what is now enforced rather than described

Five of the seven steps above used to be prose that nothing checked. `corpus/preregistration.json`
is the machine-readable half of this document, and `corpus/classification.py` reads it:

| was described | is now enforced |
|---|---|
| the draw that selects the sample | `preregistration.json` owns `population`/`per_repo`/`limit`/`minimum`; the artifact may not carry `draws` at all, and the profile is named by the **caller** of `verify` |
| "a repo that emits nothing contributes nothing" | a draw that selects zero findings is a **hard error**, never `unavailable` |
| the run measured the whole tree | `schema_version`, `complete`, `skipped`, `excluded` **and the walked `.py` count** are pinned per run and checked before a finding is read |
| the detector it was taken on | `detector_commit` must resolve to a commit in this repository, `detector_dirty` must be stated, and `binary_sha256` is checked whenever the binary is present |
| "baseline is an output, not an input" | the baseline is **derived by code from the labels** (`baseline_from`), so it cannot exist before them |
| the threshold comparison | `gate()` compares the measured share against the pre-registered number and **exits 1** when it is worse; a gate profile with a non-numeric threshold is **refused**, so the holdout cannot run until the owner sets one |

🔴 **`limit` and `minimum` are two numbers now, and that closes a loophole.** They used to be the
same value, so a half asking for 5 against a pool yielding 3 failed, and the only way out was
editing the artifact's `limit` afterwards — which made the sample size a post-run self-report. Both
come from this file, so a shortfall is recorded by the pre-registered rule rather than by the person
who saw the result.

⚠️ **What the provenance binding still cannot do, stated rather than overclaimed.** Nothing here
proves which binary produced a given JSON; only re-running it does. What it rules out is a commit
that does not exist, a dirty tree that does not admit it, and a binary on disk that is not the one
named. The residue is an attestation, and §6 already calls it that.

The artifact format is frozen by the dry run: `corpus/dry_run_classification.json` is a filled
example of it, `corpus/classification.py` is its parser and checker, and
`tests/unit/test_classification_artifact.py` is its guard, mutation-proved in both directions
(`corpus/annotations.md` §4.9).
