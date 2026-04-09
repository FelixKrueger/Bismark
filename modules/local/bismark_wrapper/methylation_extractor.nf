process METHYLATION_EXTRACTOR {
    tag "${meta.id}"
    label 'process_high'
    publishDir "${params.outdir}/bismark_methylation", mode: 'copy'

    conda "bioconda::bismark=0.24.2 bioconda::samtools=1.15.1"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("CpG_context_*.txt.gz"),   emit: cpg_calls
    tuple val(meta), path("CHG_context_*.txt.gz"),   emit: chg_calls,  optional: true
    tuple val(meta), path("CHH_context_*.txt.gz"),   emit: chh_calls,  optional: true
    tuple val(meta), path("*_splitting_report.txt"), emit: report
    tuple val(meta), path("*.M-bias.txt"),           emit: mbias

    script:
    def endedness = meta.single_end ? '--single-end' : '--paired-end'
    def no_overlap = meta.single_end ? '' : '--no_overlap'
    def comprehensive = params.comprehensive ? '--comprehensive' : ''
    def cx = params.CX_context ? '--CX' : ''
    def ignore_args = []
    if (params.ignore > 0)           { ignore_args << "--ignore ${params.ignore}" }
    if (!meta.single_end && params.ignore_r2 > 0)        { ignore_args << "--ignore_r2 ${params.ignore_r2}" }
    if (params.ignore_3prime > 0)    { ignore_args << "--ignore_3prime ${params.ignore_3prime}" }
    if (!meta.single_end && params.ignore_3prime_r2 > 0) { ignore_args << "--ignore_3prime_r2 ${params.ignore_3prime_r2}" }
    def ignore_str = ignore_args.join(' ')
    """
    bismark_methylation_extractor \\
        ${endedness} \\
        ${no_overlap} \\
        ${comprehensive} \\
        ${cx} \\
        ${ignore_str} \\
        --gzip \\
        --output . \\
        ${bam}
    """
}
