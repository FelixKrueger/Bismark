## Context change/discrepancy between Bismark coverage and genome-wide cytosine reports

A question that comes up every so often is: "Why do some positions have a different cytosine context between the coverage
and genome-wide cytosine reports produced by `coverage2cytosine`? In rare(r) cases, the same position can even be present
in several different contexts - how is this possible?"

**Answer**:

The Bismark coverage files contain every position that received a methylation call during the mapping step. There are certain
cases where the cytosine context may change due to a deletion in the immediate downstream proximity of the cytosine, like in the following example:

`CAATGGGA` Here, the first C is in CHH context. If there was a deletion of AAT, one would would get an alignment like this:

`C---GGGA` here the context would effectively have changed from CHH to CG.

At least for mammalian systems it is quite likely that such a change would also affect the methylation state of the cytosine involved
because CpGs are typically methylated whereas non-CG cytosines are largely completely unmethylated.

This context change only ever occurs for deletions immediate downstream of a cytosine, but _not insertions_. The reason for this is
that insertions are padded with `X` during the methylation call procedure, which would render the cytosine context `Unknown`.

`coverage2cytosine` on the other hand is fully reference genome-based, so it will go through the reference sequence
and check whether a cytosine was covered or not. The sequence context is purely assigned based on the reference sequence.

- For CpG context only (the default), it will therefore ‘miss’ out any of the calls where a deletion had changed the sequence
  context from `CpG` to either `CHG` or `CHH`.

- In `--CX` context: One may encounter cases where the context of a cytosine has changed, or much more rarely, where the very same position
  may have been called with different cytosine contexts in the intitial CHG, CHH and CpG-context files produced by the `bismark_methylation_extractor`.
  If now, howevever, `--CX` was used for `bismark2bedGraph`
  and `coverage2cytosine` (or directly during the `bismark_methylation_extractor` step), those _different_ cytosine contexts will be merged
  again for that position, and will get the cytosine context assigned purely based on the reference sequence. In other words: If there truly
  was a cytosine context - that may or may not affect the methylation state - it would be, probably erroneously, attributed to the context
  provided by the reference sequence.

#### In a nutshell:

By design, Bismark generally bases its methylation call behavior on the reference genome, with the rationale being that
sequencing errors occur much more frequently than true polymorphisms in the genome sequence. The sequence context of insertions
would be dependent on the basecall accuracy of the inserted base(s), so we chose to call these methylation calls in `Unknown` context.
Deletions on the other hand are a very rare error in Illumina data, and we believe it should be fine to proceed with the default behavior
of calling changed sequence context because it is very likely that a change from CHH to CpG context, or vice versa, will really
also lead to a change in that residue's methylation state.

If you work with **coverage files** in `CpG` context only you may miss a few positions that should be CpG positions according to
the reference sequence. On the other hand it may also contain a few newly gained CpGs positions that had a different context
(`CHG` or `CHH`) in the reference sequence but where the read sequence says otherwise.

If you are working in `--CX` mode, or with the genome-wide report (or both), I am afraid it is a little more complicated...
