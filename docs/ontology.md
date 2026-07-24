# Ashlar as an Ontological System

*An essay. Companion to [`philosophical_edges.md`](philosophical_edges.md), which carries the
open questions in working form; this document develops the full argument. Nothing here is
normative — the hierarchy still ends at [`vision.md`](vision.md) — but the vision's principles
turn out to be metaphysical commitments, and it is worth knowing which ones, and where they
crack.*

## 1. Two senses of "ontology"

Computer science uses "ontology" in Gruber's sense: a specification of a conceptualization. An
ontology in this sense is a catalog — classes, relations, and axioms for some domain, the kind
of thing OWL encodes and a knowledge graph instantiates. It presupposes a metaphysics and
enumerates within it.

Philosophy's sense is prior to that. Ontology there is the study of being as such: what exists,
what individuates one thing from another, what is essential to a thing and what merely happens
to it, how parts compose wholes, what depends on what, and what survives change. These are not
questions you answer by cataloging a domain. They are the questions that determine what a
catalog *could* contain.

A programming language, unusually among human artifacts, cannot avoid answering the
philosophical questions. Every language legislates what can exist (its entities), what
individuates them (its binding rules), how they compose (its composition mechanisms), and what
survives change (its refactoring story, if it has one). Most languages answer by accident,
inheriting a metaphysics piecemeal from their implementation history — which is why their
answers are incoherent, and why "what is this identifier bound to?" can require knowing a file
path, a load order, and the phase of the moon. Ashlar answers on purpose.

This is closest to Carnap's idea of a linguistic framework: to adopt a language is to adopt a
decision about what exists, and questions about what "really" exists apart from some framework
do not arise. Ashlar even gives its framework a closed world — G5's *no registry, ever* means
there is no elsewhere from which unaccounted being can arrive. So Ashlar is not an ontology
*of* something, in the CS sense. It is an ontology *generator* in the philosophical sense: a
legislated metaphysics inside which programs happen.

The thesis of this essay, in one sentence: **Ashlar is a constructed world with a one-category
ontology, nominal individuation, computable grounding, and intelligibility as a condition of
existence — a compiler-enforced principle of sufficient reason.** Part I develops that reading.
Part II breaks it, in six places. Part III shows the breaks converging on the doctrine that was
on the wall the whole time: *the build is state, the code is intent.*

---

## Part I — The professed metaphysics

### One category of being

The deepest commitment is C1: there is exactly one composable unit, and UI elements, routes,
services, state stores, and data shapes are all the same kind of thing, composed by the same
mechanism. In metaphysics this is a *one-category ontology* — the position that reality needs
only a single fundamental kind. Its most developed modern form is L.A. Paul's mereological
bundle theory: one category (qualities) plus one relation (composition) generates everything.
Ashlar is nearly a formalization of that program — one category (`part`), one relation (named
merge composition) — with a Spinozist accent: one substance, many modes. A server is not a
kind of thing; it is a part that happens to bear a `port`. Being a route, a view, a store is
not a difference in *what* something is but in *which properties it bears*. The apparent
diversity of the built world is entirely modal.

### Names individuate

What makes one part distinct from another? Not its location (B5: no source file contains a
location), not its position or order (B1), not its file (B6: `chat.ui.Message` is a name;
where it lives is a build fact). Only its name. This settles, by decree, the oldest fight in
substance metaphysics. Bundle theory says a thing just *is* its properties, and then struggles
to explain how two qualitatively identical things could be two. Substratum theory posits a
bare particular — a propertyless peg on which properties hang — and then struggles to say
anything about it. Ashlar splits the difference exactly as Armstrong's "thin particular" does:
the name is the peg. A part is its layers, flattened; but what makes it *this* part is the
name alone. Identity is primitive and nominal — a haecceity, a bare *thisness*, spelled in
dotted identifiers.

The name behaves as Kripke said proper names do: it designates rigidly, without descriptive
content. Nothing about `chat.data.Store` describes the Store; the name refers directly, which
is precisely what makes `ashlar rename` able to be total — there is no descriptive residue
scattered through the program to chase down. And B3 enforces a strong discernibility
discipline: every name resolves to exactly one definition, and B4 goes further — two names
differing only by case or separator are a compile error. It is not enough that entities be
distinct; they must be *robustly* distinguishable, even by a sloppy reader. The identity of
indiscernibles, enforced in reverse: no indiscernible names.

Naming is also *efficacious*. In most languages a name labels something constituted elsewhere.
In Ashlar, declaration is constitution: when `chat.audit` writes `part chat.data.Store`, it is
not referring to the Store but participating in its being — from a distance, without touching
the original text. This is a speech act in Austin's sense, a performative: saying it makes it
so. The older resonance is the doctrine of true names — the Adamic language, the "let there
be" — in which to name correctly is to call into being. Ashlar's slogan-level ontology could
be put as a twist on Quine: *to be is to be the value of a name.* (The §11 "anonymity
boundary" question is this doctrine's frontier: an anonymous function cannot be renamed,
moved, or referenced — E2 cannot see it. Whatever cannot be named is second-class being, and
the open decision is exactly how much nameless being the world will tolerate.)

### Essence, accident, existence

"The build is state, the code is intent" is a machine-checkable essence/accident distinction.
What is declared in source is essential to a part. Where it lives — file, path, order — is
accidental, and the accidents are not even *in* the world: locations exist only in the
manifest (B5), and F3 guarantees that relocating a file changes nothing but the recorded
accident. The sharper medieval echo is Avicenna's distinction between essence and existence:
a part's essence is in source, but its *existence* — where it actually lives, what its
flattened form actually is — is conferred by the build. And F2 (delete the manifest, rebuild
it identically) says existence is wholly determined by essence: nothing exists that essence
does not account for, and existence adds no information of its own. Kant said existence is
not a real predicate; Ashlar makes that a build invariant.

### Composition, legislated

Van Inwagen's Special Composition Question asks: under what conditions do many things compose
one thing? Philosophy has spent decades between "never," "always," and "sometimes, brutely."
Ashlar answers by decree, exhaustively: parts compose under exactly five merge kinds —
`replace`, `append`, `deep`, `stack`, `pipe` — and C4 freezes the list (a sixth is added only
by removing one). Composition is deterministic (C2) and timeless (C6: no merge outcome
depends on runtime state); given the declarations, the flattened world is the same across
runs, machines, and file layouts. Even lifecycle, the place where most systems sprout a new
ontological category, is deflated by C8 into `stack` plus a declared ordering of names. The
system resists ontological inflation the way a good metaphysician does: no new categories
where a relation among existing ones will do.

### Grounding, computed

The liveliest topic in contemporary metaphysics is *grounding* — the relation of ontological
dependence and priority that Kit Fine and Jonathan Schaffer put back at the center of the
field: what exists in virtue of what, what depends on what. Ashlar operationalizes it.
`ashlar radius` is a grounding oracle: it prints an entity's complete dependence structure,
touching nothing. E3 requires every refactor to report its blast radius — its grounding
footprint — before acting, and E5 is a genuine metaphysical stance: a refactor that cannot
compute complete blast radius refuses to run. No change may be made to an entity whose
dependence relations are indeterminate. The vision explains why this matters at scale: if an
agent generates ten times the volume a human reads, the unread portion is only trustworthy
because its grounding structure is provable. Trust in the unread is underwritten by computable
dependence.

### The principle of sufficient reason

Leibniz's principle — nothing is so without a reason why it is so rather than otherwise — is
the signature of Ashlar's build realm. Derivability (ADR-0012) minimizes semantic freedom so
the toolchain can compute and *explain* what names mean, which implementations run, and what a
change affects. C6 removes runtime contingency from composition. And C3 patrols for brute
facts: where flattening would need an arbitrary tie-break, the compiler names both parties and
suggests the declaration that would supply the missing reason. The world does not merely have
reasons; it demands them, and files a report where one is absent.

### Two realms

Composition facts are settled at build time and are eternal in the relevant sense — invariant
across runs, machines, layouts. Behavior happens at runtime, in time. This is the classic
two-realm architecture: Plato's forms and their instances, the medieval order of being and
order of becoming. The manifest sits at the boundary, a complete record of the eternal realm's
verdicts, itself fully derivable (F2) and therefore, strictly, redundant — a mirror the
temporal realm can consult.

### Intelligibility as a condition of being

Here Ashlar does something with no precedent I know of in language design. T-A3 gates syntax
on whether a *fresh mind with no context* — a model that has never seen the reference —
correctly guesses what a construct means. A misread is a design bug in the world, not an error
in the reader. This inverts the usual direction of fit between epistemology and ontology: most
systems exist first and are understood, or not, afterward; Ashlar makes knowability a
criterion of existence. A construct that cannot be guessed is not permitted to be. That is the
rationalist thesis in its strongest historical form — the real is the rational — or, in the
medieval frame, the convertibility of the transcendentals: *ens et verum convertuntur*, being
and intelligibility are one. A4 sharpens it into an ethics of appearance: false familiarity is
worse than unfamiliarity. A world may be strange, but it may not deceive; where a reader's
guess would go wrong, the world must fail loudly rather than proceed under a false
understanding.

That is the clean reading: one category, nominal individuation, essence determining existence,
legislated composition, computable grounding, an enforced PSR, two realms, and intelligibility
as the price of admission. If it held without remainder, Ashlar would be a textbook. It
doesn't, and the places it fails are the interesting part.

---

## Part II — Six strains, and a seventh

Each of the following marks a point where the analogy between Ashlar and its professed
metaphysics is under real tension. None is an accusation. A clean analogy teaches nothing; a
strained one shows where the system is actually making a decision. (These are maintained in
working form in [`philosophical_edges.md`](philosophical_edges.md), as open guidance for
ongoing design.)

**Strain 1: rename breaks the name-as-identity doctrine — from outside.** If names are the
*only* individuator, `ashlar rename` should be metaphysically impossible: renaming a part
should be death and creation, not survival. A bundle theorist who individuates by name has no
resources to say the renamed thing is "the same thing." Yet rename is atomic, reversible, and
manifestly identity-preserving — the whole point of the command is that the part persists
under a new name. Where does that persistence live? Not in the language: nothing in Ashlar
source can express "this part was formerly called X." It lives in the toolchain, during the
operation, in a change-set computed from the manifest. Identity-through-renaming exists only
transcendently — from the God's-eye position of the tool operating *on* the world, never
immanently within it. This is coherent (it is roughly how Genesis handles Abram becoming
Abraham: only the namer carries the continuity), but it means the official ontology is false
as stated. Names are the only binding mechanism *inside* the world; outside it, there is a
deeper individuator the world cannot name.

**Strain 2: hot reload smuggles in the substratum the ontology denies.** G3: source changes,
process state survives. Read metaphysically, that is startling — the part's *essence* changes
while its accidents (the runtime state, the value of `n` in the counter) carry across. In any
substance metaphysics that is backwards: accidents inhere in substance and cannot outlive a
change of form. What carries the state through the reload is neither the name (unchanged, but
names bind definitions, not process instances) nor the definition (which is precisely what
changed). The runtime has a doctrine of persistence-through-change that the build-time
ontology has no vocabulary for. The two-realm architecture has a bridge problem — the same one
Plato had: how does revision in the changeless realm propagate into the world of becoming
without destroying the becoming thing, or admitting it has an identity independent of its
form? Hot reload just *does it*, which is engineering's privilege. But the honest answer is:
there is a substratum, and it is the process.

**Strain 3: the one category isn't one.** C1 says everything composable is a part — but the
ontology quietly runs on things that are not parts. Spaces are not parts; they are regions, or
contexts. Properties are not parts; they are tropes borne by parts. Types are not parts. Most
tellingly, the five merge kinds are not parts — they are second-order relations that govern
how parts compose, and nothing composes *them*. This is exactly the objection one-category
ontologies always face: the single category needs structuring machinery, and the machinery
needs ontological standing the theory does not grant it. It is Bradley's regress in compiler
form — if names bind layers to parts, what binds the binding? Ashlar's honest description is
"one category of *composable*, plus a fixed, closed, non-composable supporting cast." Still
austere, still principled — a finite legislated superstructure rather than an open-ended one —
but a two-tier ontology, not a monism. C4's "a sixth is added only by removing one" reads as
awareness of this: the second tier is dangerous precisely because it is not self-governing, so
it is frozen by decree.

**Strain 4: the closed world has a breach, and the breach is named `unsafe`.** G5 makes Ashlar
a world with no elsewhere — no registry, no version resolution, no unaccounted arrival of
being. Everything that exists is derivable, inspectable, intelligible. Except the foreign
function boundary. `foreign/<space>.so` is a portal through which opaque causation enters: the
ontology can describe the JSON that crosses the boundary and nothing about what produces it.
This is almost too perfectly Kantian — the foreign library is the noumenon, knowable only
through its appearances at the interface — and the single `unsafe` in the codebase is the
honest marker of where intelligibility is surrendered. The strain is that Ashlar's deepest
boast, the convertibility of being and derivability, holds only up to a boundary the system
itself draws. Spinoza's substance has no outside; Ashlar's does, kept behind one door. But a
world that marks its own limit of intelligibility *in its type system* is more philosophically
serious than one that claims not to have a limit at all.

**Strain 5: intelligibility turns out to be indexed, not transcendental.** The strongest claim
in Part I was that Ashlar treats knowability as a condition of existence. But look at *whose*
knowing T-A3 measures: a fresh model, cold-reading, scored against priors formed by the
accumulated conventions of existing languages, circa now. That is not intelligibility-as-such;
it is intelligibility-to-a-particular-epistemic-community-at-a-particular-moment, and it will
drift as model priors drift. On this reading the Hegelian thesis collapses toward Protagoras —
the model is the measure of all things — or, more charitably, toward pragmatism: true syntax
is what the community of inquirers converges on. A4 quietly concedes the point: a world that
must *police its own misreadings* is admitting that being and intelligibility come apart, and
building institutions to manage the gap.

There is a counter-reading, and it may win: indexing intelligibility to actual readers is not
a retreat from rationalism but its only legitimate form. The caricature of Hegel has "the real
is the rational" as a timeless equation; the actual Hegel held that reason exists only as
historically embodied — Geist develops through communities, and there is no view from nowhere
for intelligibility to be measured against. Peirce made the same move operational: truth is
what the community of inquiry would converge on, and there is no other kind. On this reading
T-A3 is not relativism, because it is disciplined — the reader is fixed (fresh, contextless),
the corpus is fixed, the threshold is explicit, and the gate ratchets. What changes under the
counter-reading is not whether the criterion is legitimate but what kind of thing it is:
intelligibility stops being a timeless property the language *has* and becomes an achievement
the project *maintains* — a treaty with a moving community of minds, requiring renewal as
priors drift. That converts a static transcendental into an operational obligation, and it is
a design consequence, not just a gloss: T-A3 is not a proof that the syntax is guessable; it
is the instrument by which it is *kept* guessable.

**Strain 6: C3 doesn't satisfy the principle of sufficient reason — it confesses violations of
it.** When two sources are unordered by any declared relation, the compiler still *picks* an
order — deterministically — and warns. Determinism is not sufficient reason. A choice that is
repeatable but arbitrary is Buridan's ass resolved by fiat: there is no reason why this order
rather than the other, only a rule guaranteeing the same fiat every time. Leibniz would call
the warning an IOU, not payment. What Ashlar actually implements is subtler than the PSR: a
world where brute facts are permitted but must be *confessed*, each accompanied by an
invitation to legislate it away. The arbitrary is quarantined and labeled rather than
eliminated. There is no canonical name for that position — which is a good sign, because it
means the system is staking out ground rather than instantiating a textbook.

**The seventh, half strain and half feature: nobody owns a part.** The signature move —
extending `chat.data.Store` from `chat.audit` without touching the original — means the being
of an entity is distributed across every space that names it. Under substance metaphysics that
is a strain: a substance whose properties strangers can extend at a distance is not much of a
substance. But under Searle's institutional ontology it is exactly right: parts are like
corporations or currencies — entities constituted by declarations from many parties, where the
flattened result is the real thing and no single author intended it. The dark edge of the
true-names doctrine lives here too: to know the name is to have power over the thing. The
verdict this essay takes: this is the thesis, not the bug. Ashlar is a language whose authors
are many agents with no prior context and no standing to demand an ownership hierarchy;
authorship of any real system was already distributed before the language formalized it. The
language did not create the condition — it made the condition legible, gave it a deterministic
flattening order, and made every act of extension-at-a-distance a named, inspectable
declaration rather than a monkey-patch. Institutional being, with a land registry.

---

## Part III — What the strains converge on

Run down the list and a pattern appears. Identity across renames lives in the toolchain.
Persistence across reloads lives in the runtime. The second-tier categories live in the
compiler's fixed machinery. The FFI's opacity is fenced by the runtime's one `unsafe`. The
reader's historical situation lives in the test suite. The confessed tie-break lives in a
warning. Every strain marks a place where *the tooling or the runtime holds something the
language officially denies.* Not one of the six is a fact about Ashlar source.

The conclusion is not that the ontology fails. It is that the ontology was never located where
the slogans put it. Ashlar's real metaphysics lives in the whole system — language, build,
toolchain, runtime, test suites — and the language is only its innermost, most legislated
region. The language is the immanent world: everything inside it is nominal, derivable,
reasoned, and flat. The toolchain is that world's transcendent operator: rename, move, rekind,
and fix are interventions from outside the order of nature — miracles, in Hume's strict sense —
except that these miracles are themselves law-governed. E1 through E5 are a constitution for
the God's-eye position: the transcendent operator must announce its blast radius, act
atomically or not at all, leave no stale trace, and be reversible to the byte. A theology with
a written constitution and an audit log.

And the vision predicted this, in the principle that sounded most like an implementation note:
*the build is state, the code is intent.* Read ontologically, that sentence already concedes
that the world's being is not contained in its source — that what things *are* (flattened,
located, resolved, actual) is held by the build, on behalf of a source that holds only what
things *mean*. The strains are that sentence's consequences arriving in metaphysical dress,
one subsystem at a time. The essay's thesis can now be restated in its honest form: **Ashlar's
source is an ontology of intent; its system is an ontology of being; and the discipline of the
project is that the second must remain fully derivable from the first.** F2 — delete the
manifest and rebuild it identically — is not a caching guarantee. It is the whole metaphysics
in one test: existence adds nothing to essence, *and we check.*

T-META's rule — a requirement with no test is not a requirement — applies with full force here: a metaphysics with no test is just marketing. Ashlar's metaphysical commitments are unusually falsifiable. 
One category is a compile-time fact. Nominal individuation is T-B. Legislated composition is T-C's exhaustive
matrix. Computable grounding is T-E. Essence-determines-existence is T-F. Intelligibility as
maintained achievement is T-A3, re-run as the community of readers drifts. The strains
themselves are testable in principle — the seventh is *already* exercised every time the suite
drives `examples/chat`.

