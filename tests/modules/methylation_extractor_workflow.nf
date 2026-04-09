nextflow.enable.dsl = 2

include { METHYLATION_EXTRACTOR } from '../../modules/local/bismark_wrapper/methylation_extractor'

process PREPARE_AND_ALIGN {
    tag "prepare_and_align"
    container 'quay.io/biocontainers/bismark:0.24.2--hdfd78af_0'

    input:
    val(meta)
    path fasta
    path r1
    path r2

    output:
    tuple val(meta), path("*.bam"), emit: bam

    script:
    """
    gunzip -c ${fasta} > genome.fa
    mkdir genome_dir
    mv genome.fa genome_dir/
    bismark_genome_preparation --bowtie2 genome_dir
    bismark --bowtie2 --bam --genome genome_dir -1 ${r1} -2 ${r2}
    """
}

workflow METHYLATION_EXTRACTOR_TEST {
    take:
    fasta
    r1
    r2

    main:
    def meta = [id: 'test', single_end: false]
    PREPARE_AND_ALIGN(meta, fasta, r1, r2)
    METHYLATION_EXTRACTOR(PREPARE_AND_ALIGN.out.bam)

    emit:
    cpg_calls = METHYLATION_EXTRACTOR.out.cpg_calls
    report    = METHYLATION_EXTRACTOR.out.report
    mbias     = METHYLATION_EXTRACTOR.out.mbias
}
