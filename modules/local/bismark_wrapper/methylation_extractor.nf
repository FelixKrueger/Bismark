process METHYLATION_EXTRACTOR {
    tag "${meta.id}"
    label 'process_high'
    publishDir "${params.outdir}/bismark_methylation", mode: 'copy'

    conda "bioconda::bismark=0.24.2 bioconda::samtools=1.15.1"
    container "${ workflow.containerEngine == 'singularity' && !task.ext.singularity_pull_docker_container ?
        'https://depot.galaxyproject.org/singularity/bismark:0.24.2--hdfd78af_0':
        'quay.io/biocontainers/bismark:0.24.2--hdfd78af_0' }"

    input:
    tuple val(meta), path(bam)

    output:
    tuple val(meta), path("CpG_*.txt.gz"),            emit: cpg_calls
    tuple val(meta), path("CHG_*.txt.gz"),            emit: chg_calls,     optional: true
    tuple val(meta), path("CHH_*.txt.gz"),            emit: chh_calls,     optional: true
    tuple val(meta), path("*_splitting_report.txt"),  emit: report
    tuple val(meta), path("*.M-bias.txt"),            emit: mbias
    tuple val(meta), path("*.M-bias_R1.png"),         emit: mbias_r1_plot, optional: true
    tuple val(meta), path("*.M-bias_R2.png"),         emit: mbias_r2_plot, optional: true

    script:
    def args = task.ext.args ?: ''
    def endedness = meta.single_end ? '--single-end' : '--paired-end'
    def comprehensive = params.comprehensive ? '--comprehensive' : ''
    def no_overlap = meta.single_end ? '' : '--no_overlap'
    def cx = params.CX_context ? '--CX' : ''
    def cores = task.cpus > 1 ? "--multicore ${task.cpus}" : ''
    def ignore_args = ''
    if (params.ignore)           { ignore_args += " --ignore ${params.ignore}" }
    if (params.ignore_r2)        { ignore_args += " --ignore_r2 ${params.ignore_r2}" }
    if (params.ignore_3prime)    { ignore_args += " --ignore_3prime ${params.ignore_3prime}" }
    if (params.ignore_3prime_r2) { ignore_args += " --ignore_3prime_r2 ${params.ignore_3prime_r2}" }
    """
    bismark_methylation_extractor \\
        ${endedness} \\
        ${comprehensive} \\
        ${no_overlap} \\
        ${cx} \\
        ${cores} \\
        ${ignore_args} \\
        --gzip \\
        --output . \\
        ${args} \\
        ${bam}
    """
}
