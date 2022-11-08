# Running Bismark

Running Bismark is split up into three main steps:

1. First, the genome of interest needs to be bisulfite converted and indexed to allow Bowtie alignments. This step needs to be carried out only once for each genome. Note that Bowtie 2 and HISAT2 require distinct indexing steps since their indexes are not compatible.
2. Bismark read alignment step. Simply specify a file to be analysed, a reference genome and alignment parameters. Bismark will produce a combined alignment/methylation call output (default is BAM format) as well as a run statistics report.
3. Bismark methylation extractor. This step is optional and will extract the methylation information from the Bismark alignment output. Running this additional step allows splitting the methylation information up into the different contexts, getting strand-specific methylation information and offers some filtering options. You can also choose to sort the methylation information into `bedGraph`/`coverage` files, or even process them further to genome-wide cytosine methylation reports.

Each of these steps will be described in more detail (with examples) in the following sections.
