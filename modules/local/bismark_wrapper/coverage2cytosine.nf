process COVERAGE2CYTOSINE {
    tag "${meta.id}"
    label 'process_high'
    publishDir "${params.outdir}/bismark_cytosine_report", mode: 'copy'

    conda "bioconda::bismark=0.24.2"
    container "${ workflow.containerEngine == 'singularity' && !task.ext.singularity_pull_docker_container ?
        'https://depot.galaxyproject.org/singularity/bismark:0.24.2--hdfd78af_0':
        'quay.io/biocontainers/bismark:0.24.2--hdfd78af_0' }"

    input:
    tuple val(meta), path(coverage_file)
    path genome_folder

    output:
    tuple val(meta), path("*.CpG_report.txt*"),  emit: cpg_report, optional: true
    tuple val(meta), path("*.CX_report.txt*"),   emit: cx_report, optional: true

    script:
    def args = task.ext.args ?: ''
    def prefix = task.ext.prefix ?: "${meta.id}"
    def cx = params.CX_context ? '--CX' : ''
    def merge = params.merge_CpGs ? '--merge_CpGs' : ''
    """
    coverage2cytosine \\
        --genome_folder ${genome_folder} \\
        --output ${prefix} \\
        --dir . \\
        ${cx} \\
        ${merge} \\
        --gzip \\
        ${args} \\
        ${coverage_file}
    """
}
