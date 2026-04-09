#!/usr/bin/env nextflow

nextflow.enable.dsl = 2

// Import subworkflows
include { BISMARK_GENOME_PREPARATION_WF } from './subworkflows/bismark_genome_preparation'
include { BISMARK_ALIGNMENT_NATIVE      } from './subworkflows/local/bismark_alignment_native'
include { BISMARK_METHYLATION           } from './subworkflows/local/bismark_methylation'

workflow {
    // Create a channel for input reads
    input_reads = Channel
        .fromFilePairs(params.reads, checkIfExists: true)
        .map { group_id, files -> [[id: group_id, single_end: files.size() == 1], files] }

    // Run the genome preparation workflow
    BISMARK_GENOME_PREPARATION_WF(
        params.fasta,
        params.aligner,
    )

    // Run the native implementation
    BISMARK_ALIGNMENT_NATIVE(
        input_reads,
        BISMARK_GENOME_PREPARATION_WF.out.bisulfite_genome,
        params.fasta,
        params.directional,
    )

    // Methylation processing pipeline
    BISMARK_METHYLATION(
        BISMARK_ALIGNMENT_NATIVE.out.bam,
        params.fasta,
    )
}
