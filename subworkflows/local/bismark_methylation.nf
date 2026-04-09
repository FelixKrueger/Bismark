include { DEDUPLICATE_BISMARK   } from '../../modules/local/bismark_wrapper/deduplicate'
include { METHYLATION_EXTRACTOR } from '../../modules/local/bismark_wrapper/methylation_extractor'
include { BISMARK2BEDGRAPH      } from '../../modules/local/bismark2bedgraph'
include { COVERAGE2CYTOSINE     } from '../../modules/local/bismark_wrapper/coverage2cytosine'

workflow BISMARK_METHYLATION {
    take:
    bam           // channel: [ val(meta), path(bam) ]
    genome_folder // path to genome FASTA directory

    main:
    if (params.skip_deduplication) {
        ch_dedup_bam = bam
    } else {
        DEDUPLICATE_BISMARK(bam)
        ch_dedup_bam = DEDUPLICATE_BISMARK.out.bam
    }

    METHYLATION_EXTRACTOR(ch_dedup_bam)

    BISMARK2BEDGRAPH(
        METHYLATION_EXTRACTOR.out.cpg_calls,
        params.CX_context ?: false
    )

    if (params.cytosine_report) {
        COVERAGE2CYTOSINE(
            BISMARK2BEDGRAPH.out.coverage,
            genome_folder
        )
    }

    emit:
    bam       = ch_dedup_bam
    cpg_calls = METHYLATION_EXTRACTOR.out.cpg_calls
    report    = METHYLATION_EXTRACTOR.out.report
    mbias     = METHYLATION_EXTRACTOR.out.mbias
    bedgraph  = BISMARK2BEDGRAPH.out.bedgraph
    coverage  = BISMARK2BEDGRAPH.out.coverage
}
