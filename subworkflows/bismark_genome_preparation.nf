#!/usr/bin/env nextflow

include { BISMARK_CONVERT_GENOME } from '../modules/local/bismark_convert_genome'
include { BISMARK_INDEX_GENOME } from '../modules/local/bismark_index_genome'

// Define the workflow
workflow BISMARK_GENOME_PREPARATION_WF {
    take:
    fasta
    aligner     // Aligner to use (bowtie2, hisat2, minimap2)

    main:
    BISMARK_CONVERT_GENOME(
        fasta,
        params.slam ?: false
    )
    
    BISMARK_INDEX_GENOME(
        BISMARK_CONVERT_GENOME.out.bisulfite_dir,
        aligner,
        params.parallel ?: 1,  // Default to 1 if not set
        params.path_to_aligner ?: false,  // Default to false if not set
        params.large_index ?: false  // Default to false if not set
    )

    emit:
    bisulfite_genome = BISMARK_INDEX_GENOME.out.bisulfite_genome
} 