process BISMARK_CONVERT_GENOME {
    tag "${fasta.getName()}"
    label 'process_medium'

    input:
    path fasta
    val slam

    output:
    path "Bisulfite_Genome", emit: bisulfite_dir

    script:
    // SLAM mode: T->C and A->G; normal bisulfite: C->T and G->A
    def ct_from = slam ? "T" : "C"
    def ct_to   = slam ? "C" : "T"
    def ga_from = slam ? "A" : "G"
    def ga_to   = slam ? "G" : "A"
    def cat_cmd = fasta.name.endsWith('.gz') ? "gunzip -c ${fasta}" : "cat ${fasta}"

    """
    mkdir -p Bisulfite_Genome/CT_conversion Bisulfite_Genome/GA_conversion

    ${cat_cmd} | awk \\
        -v ct_from="${ct_from}" -v ct_to="${ct_to}" \\
        -v ga_from="${ga_from}" -v ga_to="${ga_to}" \\
        -v ct_file="Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa" \\
        -v ga_file="Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa" '
    /^>/ {
        split(\$0, a, " ")
        name = substr(a[1], 2)
        print ">" name "_CT_converted" > ct_file
        print ">" name "_GA_converted" > ga_file
        next
    }
    {
        seq = toupper(\$0)
        gsub(/[^ATCGN]/, "N", seq)
        ct = seq; gsub(ct_from, ct_to, ct); print ct > ct_file
        ga = seq; gsub(ga_from, ga_to, ga); print ga > ga_file
    }'
    """
}
