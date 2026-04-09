process DEDUPLICATE_BISMARK {
    tag "${meta.id}"
    label 'process_medium'
    publishDir "${params.outdir}/bismark_deduplicated", mode: 'copy'

    conda "bioconda::bismark=0.24.2 bioconda::samtools=1.15.1"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("*.deduplicated.bam"),         emit: bam
    tuple val(meta), path("*.deduplication_report.txt"), emit: report

    script:
    def endedness = meta.single_end ? '--single' : '--paired'
    """
    deduplicate_bismark ${endedness} --bam --output_dir . ${bam}
    """
}
