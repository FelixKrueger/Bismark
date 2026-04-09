process COVERAGE2CYTOSINE {
    tag "${meta.id}"
    label 'process_high'
    publishDir "${params.outdir}/bismark_cytosine_report", mode: 'copy'

    conda "bioconda::bismark=0.24.2"

    input:
    tuple val(meta), path(coverage_file)
    path genome_folder

    output:
    tuple val(meta), path("*.CpG_report.txt*"), emit: cpg_report
    tuple val(meta), path("*.CX_report.txt*"),  emit: cx_report, optional: true

    script:
    def cx = params.CX_context ? '--CX' : ''
    def merge = params.merge_CpGs ? '--merge_CpGs' : ''
    """
    coverage2cytosine \\
        --genome_folder ${genome_folder} \\
        --output ${meta.id} \\
        --dir . \\
        ${cx} \\
        ${merge} \\
        --gzip \\
        ${coverage_file}
    """
}
