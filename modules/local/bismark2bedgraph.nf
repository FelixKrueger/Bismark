process BISMARK2BEDGRAPH {
    tag "${meta.id}"
    label 'process_medium'
    publishDir "${params.outdir}/bismark_bedgraph", mode: 'copy'

    conda "conda-forge::python=3.9.5"

    input:
    tuple val(meta), path(methylation_calls)
    val cx_context

    output:
    tuple val(meta), path("*.bedGraph.gz"),     emit: bedgraph
    tuple val(meta), path("*.bismark.cov.gz"),  emit: coverage

    script:
    def prefix = task.ext.prefix ?: "${meta.id}"
    def cx_flag = cx_context ? '--cx' : ''
    def threshold = params.coverage_threshold ?: 1
    """
    bismark2bedgraph.py \\
        --output ${prefix}.bedGraph.gz \\
        --cutoff ${threshold} \\
        ${cx_flag} \\
        ${methylation_calls}
    """
}
