#!/usr/bin/perl --
use strict;
use warnings;
use IO::Handle;
use Cwd;
$|++;
use Getopt::Long;


## This program is Copyright (C) 2011, Felix Krueger (felix.krueger@bbsrc.ac.uk)

## This program is free software: you can redistribute it and/or modify
## it under the terms of the GNU General Public License as published by
## the Free Software Foundation, either version 3 of the License, or
## (at your option) any later version.

## This program is distributed in the hope that it will be useful,
## but WITHOUT ANY WARRANTY; without even the implied warranty of
## MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
## GNU General Public License for more details.

## You should have received a copy of the GNU General Public License
## along with this program.  If not, see <http://www.gnu.org/licenses/>.


my $parent_dir = getcwd;
my $ASAP_version = 'v0.1.0';
my $genome_folder;

### before processing the command line we will replace --solexa1.3-quals with --phred64-quals as the '.' in the option name will cause Getopt::Long to fail
foreach my $arg (@ARGV){
  if ($arg eq '--solexa1.3-quals'){
    $arg = '--phred64-quals';
  }
}
my @filenames;   # will be populated by processing the command line

my ($genome_index_basename_1,$genome_index_basename_2,$genome_1,$genome_2,$path_to_bowtie,$sequence_file_format,$bowtie_options,$unmapped,$dissimilar,$phred64,$solexa) = process_command_line();

my @fhs;         # stores alignment process names, genome index location, bowtie filehandles and the number of times sequences produced an alignment
my %counting;    # counting various events

my %genome_1;
my %genome_2;
my %chromosomes;

foreach my $filename (@filenames){
  chdir $parent_dir or die "Unable to move to initial working directory $!\n";

  ### resetting the counting hash and fhs
  reset_counters_and_fhs();

  ### PAIRED-END ALIGNMENTS
  if ($filename =~ ','){
    print "\nPaired-end alignments will be performed\n",'='x39,"\n\n";

    my ($filename_1,$filename_2) = (split (/,/,$filename));
    print "The provided filenames for paired-end alignments are $filename_1 and $filename_2\n";

    $fhs[0]->{inputfile_1} = $filename_1;
    $fhs[0]->{inputfile_2} = $filename_2;
    $fhs[1]->{inputfile_1} = $filename_1;
    $fhs[1]->{inputfile_2} = $filename_2;

    ### FastA format
    if ($sequence_file_format eq 'FASTA'){
      print "Input files are specified to be in FastA format\n";

      ### Creating 2 different bowtie filehandles and storing the first entry
      paired_end_align_fragments_fastA ($filename_1,$filename_2);
    }

    ### FastQ format
    else{
      print "Input files are in FastQ format\n";

      ### Creating 2 different bowtie filehandles and storing the first entry
      paired_end_align_fragments_fastQ ($filename_1,$filename_2);
    }
    prepare_output_files_paired_end($filename_1,$filename_2);
  }

  ### else we are performing SINGLE-END ALIGNMENTS
  else{
    print "\nSingle-end alignments will be performed\n",'='x39,"\n\n";

    $fhs[0]->{inputfile} = $fhs[1]->{inputfile} = $filename;

    ### FastA format
    if ($sequence_file_format eq 'FASTA'){
      print "Input file is specified to be in FastA format\n";

      ### Creating 2 different bowtie filehandles and storing the first entry
      single_end_align_fragments_fastA ($filename);
    }

    ## FastQ format
    else{
      print "Input file is in FastQ format\n";

      ### Creating 2 different bowtie filehandles and storing the first entry
      single_end_align_fragments_fastQ ($filename);
    }
    prepare_output_files_single_end($filename);
  }
}



#######################################################################################################################################
### Prepare output filehandles single-end


sub prepare_output_files_single_end {
  my ($sequence_file) = @_;

  ### we print alignments to 3 result files:
  ### (1) genome 1-specific alignments
  ### (2) genome 2-specific alignments
  ### (3) aligned to both equally well

  if ($unmapped){
    open (UNMAPPED,'>',$unmapped) or die "Failed to write to $unmapped: $!\n";
    print UNMAPPED "Unmapped_reads ASAP version: $ASAP_version\talignments to both $genome_index_basename_1 and $genome_index_basename_2\n";
    print "Unmapped sequences will be written to $unmapped\n";
  }

  ### creating outfiles
  my $outfile_genome_1 = my $outfile_genome_2 = my $outfile_mixed = $sequence_file;

  $outfile_genome_1 =~ s/$/_g1_specific_ASAP.txt/;
  print "Writing genome 1 specific alignments to $outfile_genome_1\n";
  open (OUT_G1,'>',$outfile_genome_1) or die "Failed to write to $outfile_genome_1: $!\n";
  print OUT_G1 "ASAP version: $ASAP_version\t$genome_index_basename_1\n";

  $outfile_genome_2 =~ s/$/_g2_specific_ASAP.txt/;
  print "Writing genome 2 specific alignments to $outfile_genome_2\n";
  open (OUT_G2,'>',$outfile_genome_2) or die "Failed to write to $outfile_genome_2: $!\n";
  print OUT_G2 "ASAP version: $ASAP_version\t$genome_index_basename_2\n";

  $outfile_mixed =~ s/$/_common_alignments_ASAP.txt/;
  print "Writing common alignments to $outfile_mixed\n\n";
  open (OUT_MIXED,'>',$outfile_mixed) or die "Failed to write to $outfile_mixed: $!\n";
  print OUT_MIXED "ASAP version: $ASAP_version\talignments to both $genome_index_basename_1 and $genome_index_basename_2\n";

  ### printing alignment summary to a report file
  my $reportfile = $sequence_file;
  $reportfile =~ s/$/_report_ASAP.txt/;
  open (REPORT,'>',$reportfile) or die "Failed to write to $reportfile: $!\n";
  print REPORT "ASAP analysis of file: $sequence_file\n\n";
  print REPORT "Bowtie was run against the genomes\ngenome 1: $genome_index_basename_1\ngenome 2: $genome_index_basename_2\nusing options: $bowtie_options\n\n";

  ### if 2 or more files are provided we can hold the genome in memory and don't need to read it in a second time
  unless (%genome_1 and %genome_2){
    read_genome_1_into_memory($parent_dir);
    read_genome_2_into_memory($parent_dir);
  }

  ### Input file is in FastA format
  if ($sequence_file_format eq 'FASTA'){
    process_single_end_fastA_file($sequence_file);
  }
  ### Input file is in FastQ format
  else{
    process_single_end_fastQ_file($sequence_file);
  }
}


#######################################################################################################################################
### Prepare output filehandles paired-end


sub prepare_output_files_paired_end {

  my ($sequence_file_1,$sequence_file_2) = @_;

  ### we print alignments to 3 result files:
  ### (1) genome 1-specific alignments
  ### (2) genome 2-specific alignments
  ### (3) aligned to both equally well

  ### creating outfiles
  my $outfile_genome_1 = my $outfile_genome_2 = my $outfile_mixed = $sequence_file_1;

  if ($unmapped){
    my $unmapped_1 = my $unmapped_2 = $unmapped;
    $unmapped_1 =~ s/(\.\w+)$/_1$1/;
    $unmapped_2 =~ s/(\.\w+)$/_2$1/;

    open (UNMAPPED_1,'>',$unmapped_1) or die "Failed to write to $unmapped_1: $!\n";
    print UNMAPPED_1 "Unmapped_reads ASAP version: $ASAP_version\talignments to both $genome_index_basename_1 and $genome_index_basename_2\n";
    print "Unmapped sequences will be written to $unmapped_1\n";

    open (UNMAPPED_2,'>',$unmapped_2) or die "Failed to write to $unmapped_2: $!\n";
    print UNMAPPED_2 "Unmapped_reads ASAP version: $ASAP_version\talignments to both $genome_index_basename_1 and $genome_index_basename_2\n";
    print "Unmapped sequences will be written to $unmapped_2\n";
  }

  $outfile_genome_1 =~ s/$/_g1_specific_pe_ASAP.txt/;
  print "Writing genome 1 specific alignments to $outfile_genome_1\n";
  open (OUT_G1,'>',$outfile_genome_1) or die "Failed to write to $outfile_genome_1: $!\n";
  print OUT_G1 "ASAP version: $ASAP_version\t$genome_index_basename_1\n";

  $outfile_genome_2 =~ s/$/_g2_specific_pe_ASAP.txt/;
  print "Writing genome 2 specific alignments to $outfile_genome_2\n";
  open (OUT_G2,'>',$outfile_genome_2) or die "Failed to write to $outfile_genome_2: $!\n";
  print OUT_G2 "ASAP version: $ASAP_version\t$genome_index_basename_2\n";

  $outfile_mixed =~ s/$/_common_alignments_pe_ASAP.txt/;
  print "Writing common alignments to $outfile_mixed\n\n";
  open (OUT_MIXED,'>',$outfile_mixed) or die "Failed to write to $outfile_mixed: $!\n";
  print OUT_MIXED "ASAP version: $ASAP_version\talignments to both $genome_index_basename_1 and $genome_index_basename_2\n";

  ### printing alignment summary to a report file
  my $reportfile = $sequence_file_1;
  $reportfile =~ s/$/_paired-end_report_ASAP.txt/;
  open (REPORT,'>',$reportfile) or die "Failed to write to $reportfile: $!\n";
  print REPORT "ASAP report for: $sequence_file_1 and $sequence_file_2\n";
  print REPORT "Bowtie was run against the genomes\ngenome 1: $genome_index_basename_1\ngenome 2:$genome_index_basename_2\nwith the Bowtie options: $bowtie_options\n\n";

  read_genome_1_into_memory($parent_dir);
  read_genome_2_into_memory($parent_dir);

  ### Input files are in FastA format
  if ($sequence_file_format eq 'FASTA'){
    process_paired_end_fastA_files($sequence_file_1,$sequence_file_2);
  }
  ### Input files are in FastQ format
  else{
    process_paired_end_fastQ_files($sequence_file_1,$sequence_file_2);
  }
}


#######################################################################################################################################
### Processing sequence files (single-end)

sub process_single_end_fastA_file{

  my ($sequence_file) = @_;

  ### Now reading in the sequence file sequence by sequence and see if the current sequence was mapped to one or both of the two genomes
  open (IN,$sequence_file) or die $!;
  warn "\nReading in the sequence file $sequence_file\n";
  while (1) {

    my $identifier = <IN>;
    my $sequence = <IN>;

    last unless ($identifier and $sequence);

    $counting{sequences_count}++;
    if ($counting{sequences_count}%1000000==0) {
      warn "Processed $counting{sequences_count} sequences so far\n";
    }
    chomp $sequence;
    chomp $identifier;

    $identifier =~ s/^>//; # deletes the > at the beginning of FastA headers

    ### check if there is a valid alignment
    my $return = check_bowtie_results_single_end(uc$sequence,$identifier);

    unless ($return){
      $return = 0;
    }

    # print the sequence to unmapped.out if --un was specified
    if ($unmapped and $return == 1){
      print UNMAPPED ">$identifier\n";	
      print UNMAPPED "$sequence\n";
    }
  }
  warn "Processed $counting{sequences_count} sequences from $sequence_file in total\n\n";
  close IN or die "Failed to close filehandle $!";
  print_final_analysis_report_single_end();
}

sub process_single_end_fastQ_file{

  my ($sequence_file) = @_;

  ### Now reading in the sequence file sequence by sequence and see if the current sequence was mapped to one or both of the two genomes
  open (IN,$sequence_file) or die $!;
  warn "\nReading in the sequence file $sequence_file\n";

  while (1) {

    my $identifier = <IN>;
    my $sequence = <IN>;
    my $identifier_2 = <IN>;
    my $quality_value = <IN>;

    last unless ($identifier and $sequence and $identifier_2 and $quality_value);
    #  last if ($counting{sequences_count} == 10);

    $counting{sequences_count}++;
    if ($counting{sequences_count}%1000000==0) {
      warn "Processed $counting{sequences_count} sequences so far\n";
    }

    chomp $sequence;
    chomp $identifier;
    chomp $quality_value;

    $identifier =~ s/^\@//;	# deletes the @ at the beginning of Illumina FastQ headers

    ### check if there is a valid alignment
    my $return = check_bowtie_results_single_end(uc$sequence,$identifier,$quality_value);

    unless ($return){
      $return = 0;
    }

    # print the sequence to unmapped.out if --un was specified
    if ($unmapped and $return == 1){
      print UNMAPPED '@',"$identifier\n";	
      print UNMAPPED "$sequence\n";
      print UNMAPPED $identifier_2;	
      print UNMAPPED "$quality_value\n";
    }
  }

  warn "Processed $counting{sequences_count} sequences from $sequence_file in total\n\n";
  close IN or die "Failed to close filehandle $!";

  print_final_analysis_report_single_end();
}


#######################################################################################################################################
### Processing sequence files (paired-end)


sub process_paired_end_fastA_files{

  my ($sequence_file_1,$sequence_file_2) = @_;

  ### The sequence identifier per definition needs to be the same for a sequence pair used for paired-end mapping.
  ### Now reading in the sequence files sequence by sequence and see if the current sequences produced an alignment to one or both of the two genomes

  open (IN1,$sequence_file_1) or die $!;
  open (IN2,$sequence_file_2) or die $!;
  warn "\nReading in the sequence files $sequence_file_1 and $sequence_file_2\n";

  ### Both files are required to have the exact same number of sequences, therefore we can process the sequences jointly one by one
  while (1) {

    # reading from the first input file
    my $identifier_1 = my $orig_identifier_1 = <IN1>;
    my $sequence_1 = <IN1>;

    # reading from the second input file
    my $identifier_2 = my $orig_identifier_2 = <IN2>;
    my $sequence_2 = <IN2>;

    last unless ($identifier_1 and $sequence_1 and $identifier_2 and $sequence_2);

    $counting{sequences_count}++;
    if ($counting{sequences_count}%1000000==0) {
      warn "Processed $counting{sequences_count} sequences so far\n";
    }

    chomp $sequence_1;
    chomp $identifier_1;
    chomp $sequence_2;
    chomp $identifier_2;

    $identifier_1 =~ s/^>//; # deletes the > at the beginning of FastA headers
    $identifier_2 =~ s/^>//;
    $identifier_1 =~ s/\/[12]//; # deletes the 1/2 at the end
    $identifier_2 =~ s/\/[12]//;

    if ($identifier_1 eq $identifier_2){
      my $return = check_bowtie_results_paired_ends(uc$sequence_1,uc$sequence_2,$identifier_1);

      unless ($return){
	$return = 0;
      }

      # print the sequence to unmapped_1.out and unmapped_2.out if --un was specified
      if ($unmapped and $return == 1){
	print UNMAPPED_1 $orig_identifier_1;	
	print UNMAPPED_1 "$sequence_1\n";

	print UNMAPPED_2 $orig_identifier_2;	
	print UNMAPPED_2 "$sequence_2\n";
      }
    }
    else {
      die "Sequence IDs are not identical for the sequences: $identifier_1\t$identifier_2\n";
    }
  }
  print "Processed $counting{sequences_count} sequences in total\n\n";
  close IN1 or die "Failed to close filehandle $!";
  close IN2 or die "Failed to close filehandle $!";
  print_final_analysis_report_paired_end();
}

sub process_paired_end_fastQ_files{
  my ($sequence_file_1,$sequence_file_2) = @_;

  ### The sequence identifier per definition needs to be same for a sequence pair used for paired-end alignments.
  ### Now reading in the sequence files sequence by sequence and see if the current sequences produced a paired-end alignment to one or both of the two genomes

  open (IN1,$sequence_file_1) or die $!;
  open (IN2,$sequence_file_2) or die $!;
  warn "\nReading in the sequence files $sequence_file_1 and $sequence_file_2\n";

  ### Both files are required to have the exact same number of sequences, therefore we can process the sequences jointly one by one
  while (1) {

    # reading from the first input file
    my $identifier_1 = my $orig_identifier_1 = <IN1>;
    my $sequence_1 = <IN1>;
    my $ident_1 = <IN1>;         # not needed
    my $quality_value_1 = <IN1>;

    # reading from the second input file
    my $identifier_2 = my $orig_identifier_2 = <IN2>;
    my $sequence_2 = <IN2>;
    my $ident_2 = <IN2>;         # not needed
    my $quality_value_2 = <IN2>;

    last unless ($identifier_1 and $sequence_1 and $ident_1 and $quality_value_1 and $identifier_2 and $sequence_2 and $ident_2 and $quality_value_2);

    $counting{sequences_count}++;
    if ($counting{sequences_count}%1000000==0) {
      warn "Processed $counting{sequences_count} sequences so far\n";
    }

    chomp $sequence_1;
    chomp $identifier_1;
    chomp $quality_value_1;

    chomp $sequence_2;
    chomp $identifier_2;
    chomp $quality_value_2;

    $identifier_1 =~ s/^\@//;	 # deletes the @ at the beginning of Illumina FastQ headers
    $identifier_2 =~ s/^\@//;
    $identifier_1 =~ s/\/[12]//; # deletes the 1/2 at the end
    $identifier_2 =~ s/\/[12]//;

    if ($identifier_1 eq $identifier_2){
      my $return = check_bowtie_results_paired_ends(uc$sequence_1,uc$sequence_2,$identifier_1,$quality_value_1,$quality_value_2);

      # print the sequence to unmapped_1.out and unmapped_2.out if --un was specified
      if ($unmapped and $return == 1){
	# seq_1
	print UNMAPPED_1 $orig_identifier_1;	
	print UNMAPPED_1 "$sequence_1\n";
	print UNMAPPED_1 $ident_1;	
	print UNMAPPED_1 "$quality_value_1\n";
	# seq_2
	print UNMAPPED_2 $orig_identifier_2;	
	print UNMAPPED_2 "$sequence_2\n";
	print UNMAPPED_2 $ident_2;	
	print UNMAPPED_2 "$quality_value_2\n";
      }
    }

    else {
      die "Sequence IDs are not identical for sequences: $identifier_1\t$identifier_2\n $!";
    }

  }

  print "Processed $counting{sequences_count} sequences in total\n\n";
  close IN1 or die "Failed to close filehandle $!";
  close IN2 or die "Failed to close filehandle $!";

  print_final_analysis_report_paired_end();
}


sub print_final_analysis_report_single_end{

  print REPORT "Final Alignment report\n",'='x22,"\n";
  print "Final Alignment report\n",'='x22,"\n";

  if ($dissimilar){
    print "\nDissimilar genomes were selected. Sequences aligning equally well to both genomes will not be printed out\n\n";
    print REPORT "\nDissimilar genomes were selected. Sequences aligning equally well to both genomes will not be printed out\n\n";
  }

  print "Sequences specific for genome 1:\t$counting{genome_1_specific_count}\n";
  print "Sequences specific for genome 2:\t$counting{genome_2_specific_count}\n";

  if ($dissimilar){
    print "\n";
    print REPORT "\n";
  }
  else{
    print "Sequences aligning equally well to both genomes:\t$counting{aligns_to_both_genomes_equally_well_count}\n\n";
  }

  print REPORT "Sequences specific for genome 1:\t$counting{genome_1_specific_count}\n";
  print REPORT "Sequences specific for genome 2:\t$counting{genome_2_specific_count}\n";

  unless ($dissimilar){
    print REPORT "Sequences aligning equally well to both genomes:\t$counting{aligns_to_both_genomes_equally_well_count}\n\n";
  }

  print "Sequences not mapping uniquely (3+ alignments which were thus discarded):\t$counting{ambiguous_mapping_count}\n";
  print "Sequences aligning equally well but to different positions in both genomes:\t$counting{unsuitable_sequence_count}\n";

  print REPORT "Sequences which did not map uniquely (3+ alignments which were thus discarded):\t$counting{ambiguous_mapping_count}\n";
  print REPORT "Sequences aligning equally well but to different positions in the both genomes:\t$counting{unsuitable_sequence_count}\n";


  print "Unable to extract genomic sequence count:\t$counting{unable_to_extract_genomic_sequence_count}";
  print REPORT "Unable to extract genomic sequence count:\t$counting{unable_to_extract_genomic_sequence_count}";

  if ($dissimilar){
    print "\nSequences aligning equally well to both dissimilar genomes (not printed out):\t$counting{equivalent_sequences_dissimilar_count}\n\n";
    print REPORT "\nSequences aligning equally well to both dissimlar genomes (not printed out):\t$counting{equivalent_sequences_dissimilar_count}\n\n";
  }
  else{
    print "\t(if this number is very high you might want to consider specifying --dissimilar)\n";
    print  REPORT "\t(if this number is very high you might want to consider specifying --dissimilar)\n";
  }

  print "Sequences with no alignments at all:\t$counting{no_single_alignment_found}\n\n";
  print REPORT "Sequences with no alignments at all:\t$counting{no_single_alignment_found}\n\n";

  print "Total sequences processed:\t$counting{sequences_count}\n\n";
  print REPORT "Total sequences processed:\t$counting{sequences_count}\n\n";

  my $percent_alignable_sequences = sprintf ("%.2f",($counting{genome_1_specific_count}+$counting{genome_2_specific_count}+$counting{aligns_to_both_genomes_equally_well_count})*100/$counting{sequences_count});
  print "Overall mapping efficiency (uniquely placeable reads):\t$percent_alignable_sequences%\n\n";
  print REPORT "Overall mapping efficiency (uniquely placeable reads):\t$percent_alignable_sequences%\n\n";
}



sub print_final_analysis_report_paired_end{

  print REPORT "Final Alignment report\n",'='x22,"\n";
  print "Final Alignment report\n",'='x22,"\n";

  if ($dissimilar){
    print "\nDissimilar genomes were selected. Sequences aligning equally well to both genomes will not be printed out\n\n";
    print REPORT "\nDissimilar genomes were selected. Sequences aligning equally well to both genomes will not be printed out\n\n";
  }

  print "Sequences specific for genome 1:\t$counting{genome_1_specific_count}\n";
  print "Sequences specific for genome 2:\t$counting{genome_2_specific_count}\n";

  if ($dissimilar){
    print "\n";
    print REPORT "\n";
  }
  else{
    print "Sequences aligning equally well to both genomes:\t$counting{aligns_to_both_genomes_equally_well_count}\n\n";
  }

  print REPORT "Sequences specific for genome 1:\t$counting{genome_1_specific_count}\n";
  print REPORT "Sequences specific for genome 2:\t$counting{genome_2_specific_count}\n";

  unless ($dissimilar){
    print REPORT "Sequences aligning equally well to both genomes:\t$counting{aligns_to_both_genomes_equally_well_count}\n\n";
  }

  print "Sequences not mapping uniquely (3+ alignments which were thus discarded):\t$counting{ambiguous_mapping_count}\n";
  print "Sequences aligning equally well but to different positions in both genomes:\t$counting{unsuitable_sequence_count}\n";

  print REPORT "Sequences which did not map uniquely (3+ alignments which were thus discarded):\t$counting{ambiguous_mapping_count}\n";
  print REPORT "Sequences aligning equally well but to different positions in the both genomes:\t$counting{unsuitable_sequence_count}\n";


  print "Unable to extract genomic sequence count:\t$counting{unable_to_extract_genomic_sequence_count}";
  print REPORT "Unable to extract genomic sequence count:\t$counting{unable_to_extract_genomic_sequence_count}";

  if ($dissimilar){
    print "\nSequences aligning equally well to both dissimilar genomes (not printed out):\t$counting{equivalent_sequences_dissimilar_count}\n\n";
    print REPORT "\nSequences aligning equally well to both dissimlar genomes (not printed out):\t$counting{equivalent_sequences_dissimilar_count}\n\n";
  }
  else{
    print "\t(if this number is very high you might want to consider specifying --dissimilar)\n";
    print  REPORT "\t(if this number is very high you might want to consider specifying --dissimilar)\n";
  }

  print "Sequences with no alignments at all:\t$counting{no_single_alignment_found}\n\n";
  print REPORT "Sequences with no alignments at all:\t$counting{no_single_alignment_found}\n\n";

  print "Total sequences processed:\t$counting{sequences_count}\n\n";
  print REPORT "Total sequences processed:\t$counting{sequences_count}\n\n";

  my $percent_alignable_sequences = sprintf ("%.2f",($counting{genome_1_specific_count}+$counting{genome_2_specific_count}+$counting{aligns_to_both_genomes_equally_well_count})*100/$counting{sequences_count});
  print "Overall mapping efficiency (uniquely placeable reads):\t$percent_alignable_sequences%\n\n";
  print REPORT "Overall mapping efficiency (uniquely placeable reads):\t$percent_alignable_sequences%\n\n";
}

#######################################################################################################################################
### Checking bowtie results (single-end)


sub check_bowtie_results_single_end{

  my ($sequence,$identifier,$quality_value) = @_;
  my %mismatches = ();

  unless ($quality_value){ # for FastA sequences do not have quality values
    $quality_value = '';
  }

  ### we will output the FastQ quality in Sanger encoding (Phred 33 scale)
  if ($phred64){
    $quality_value = convert_phred64_quals_to_phred33($quality_value);
  }
  elsif ($solexa){
    $quality_value = convert_solexa_quals_to_phred33($quality_value);
  }

  ### reading from the bowtie output filehandle to see if this sequence aligned to one of the two genomes
  foreach my $index (0..$#fhs){

    ### skipping this index if the last alignment has been set to undefined already (i.e. end of bowtie output)
    next unless ($fhs[$index]->{last_line} and $fhs[$index]->{last_seq_id});

    ### if the sequence we are currently looking at produced an alignment we are doing various things with it
    if ($fhs[$index]->{last_seq_id} eq $identifier) {

      ###############################################################
      ### STEP I Now processing the alignment stored in last_line ###
      ###############################################################

      ### extract some useful information from the Bowtie output
      chomp $fhs[$index]->{last_line};

      my ($id,$strand,$mapped_chromosome,$position,$bowtie_sequence,$mismatch_info) = (split (/\t/,$fhs[$index]->{last_line},-1))[0,1,2,3,4,7];

      ### Now extracting the number of mismatches
      my $number_of_mismatches;
      if ($mismatch_info eq ''){
	$number_of_mismatches = 0;
      }
      elsif ($mismatch_info =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info);
	$number_of_mismatches = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field:\t>>> $mismatch_info <<<\n";
      }

      ### creating a composite location variable from $mapped_chromosome, $position and $index and storing the alignment information in a temporary hash table
      my $alignment_location = join (":",$mapped_chromosome,$position,$index);

      $mismatches{$number_of_mismatches}->{$alignment_location}->{line} = $fhs[$index]->{last_line};
      $mismatches{$number_of_mismatches}->{$alignment_location}->{index} = $index;

      ######################################################################################################################################################
      ### STEP II Now reading in the next line from the bowtie filehandle. The next alignment can either be a second alignment of the same sequence or a ###
      ### a new sequence. In either case we will store the next line in @fhs ->{last_line}.                                                              ###
      ######################################################################################################################################################

      my $newline = $fhs[$index]->{fh}-> getline();
      if ($newline){
	chomp $newline;
	my ($seq_id) = split (/\t/,$newline);
	$fhs[$index]->{last_seq_id} = $seq_id;
	$fhs[$index]->{last_line} = $newline;
      }
      else {
	# assigning undef to last_seq_id and last_line and jumping to the next index (end of bowtie output)
	$fhs[$index]->{last_seq_id} = undef;
	$fhs[$index]->{last_line} = undef;
	next;
      }	

      ### If the new alignment is already the next entry we will process it further in the next round only.
      next unless ($fhs[$index]->{last_seq_id} eq $identifier);

      ### If it is a second alignment for the same ID we will continue adding it to the %mismatches hash

      ### reading the second reported alignment for a sequence. Resetting the variables to ensure they are fresh
      $id = $strand = $mapped_chromosome = $position = $bowtie_sequence = $mismatch_info = undef;
      ($id,$strand,$mapped_chromosome,$position,$bowtie_sequence,$mismatch_info) = (split (/\t/,$fhs[$index]->{last_line},-1))[0,1,2,3,4,7];

      ### Now extracting the number of mismatches
      $number_of_mismatches = undef;
      if ($mismatch_info eq ''){
	$number_of_mismatches = 0;
      }
      elsif ($mismatch_info =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info);
	$number_of_mismatches = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field\n";
      }

      ### creating a composite location variable from $mapped_chromosome, $position and $index and storing the alignment information in a temporary hash table
      $alignment_location = undef;
      $alignment_location = join (":",$mapped_chromosome,$position,$index);

      $mismatches{$number_of_mismatches}->{$alignment_location}->{line} = $fhs[$index]->{last_line};
      $mismatches{$number_of_mismatches}->{$alignment_location}->{index} = $index;

      ####################################################################################################################################
      #### STEP III Now reading in one more line which has to be the next alignment to be analysed. Adding it to @fhs ->{last_line}    ###
      ####################################################################################################################################
      $newline = $fhs[$index]->{fh}-> getline();

      if ($newline){
	chomp $newline;
	my ($seq_id) = split (/\t/,$newline);
	die "The same seq ID occurred more than twice in a row\n" if ($seq_id eq $identifier);
	$fhs[$index]->{last_seq_id} = $seq_id;
	$fhs[$index]->{last_line} = $newline;
	next;
      }	
      else {
	# assigning undef to last_seq_id and last_line and jumping to the next index (end of bowtie output)
	$fhs[$index]->{last_seq_id} = undef;
	$fhs[$index]->{last_line} = undef;
	next;
      }
    } ### still within the foreach index loop
  }

  ### if there was no single alignment for a certain sequence we will continue with the next sequence in the sequence file
  unless(%mismatches){
    $counting{no_single_alignment_found}++;
    return 1; ### We will print this sequence out as unmapped sequence if --un unmapped.out has been specified
  }

  #  foreach my $mm (sort {$a<=>$b} keys %mismatches){
  #    foreach my $alignment_position (keys %{$mismatches{$mm}} ){
  #      print "$mm\t$alignment_position\t$mismatches{$mm}->{$alignment_position}->{line}";
  #    }
  #  }
  #  print "\n";

  #######################################################################################################################################################
  ### We are now looking if there is a unique best alignment for a certain sequence. This means we are sorting in ascending order and looking at the  ###
  ### sequence with the lowest amount of mismatches.                                                                                                  ###
  #######################################################################################################################################################

  ### sort in ascending order
  for my $mismatch_number (sort {$a<=>$b} keys %mismatches){

    ### if there is only 1 entry in the hash with the lowest number of mismatches the sequence is unique to one of the genomes
    if (scalar keys %{$mismatches{$mismatch_number}} == 1){

      ### unique best alignment here is in fact the composite chromosome:position:index string
      for my $unique_best_alignment (keys %{$mismatches{$mismatch_number}}){

       	### we neeed to discriminate the following 2 cases:
	### (a) genomes are dissimilar (e.g. one genome is only a single chromosome of another species). This needs to be specified by the --dissimilar option.

	### (b) both genomes are essentially the same and differ only in a number of SNPs. This is the default option
	
	my $index = $mismatches{$mismatch_number}->{$unique_best_alignment}->{index};

	### (a) if the genomes are dissimilar we are going to write out the genome-specific alignment and it's coordinates and will also write out the
	### best alignment to the other genome and its mismatch information

	if ($dissimilar){

	  my ($id,$strand,$chr,$start,$bowtie_sequence,$mismatch_info) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line},-1))[0,1,2,3,4,7];

	  $start += 1; # bowtie alignments are 0 based
	  my $end = $start+length($sequence)-1;

	  my $genome_1_sequence;
	  my $genome_2_sequence;
	  my $mismatch_info_1;
	  my $mismatch_info_2;
	
	  if ($index == 0){
	    $genome_1_sequence = substr($genome_1{$chr},$start-1,length$sequence); # subtract -1 from the start position again
	    $mismatch_info_1 = $mismatch_info;

	    ### if the alignment came from the reverse strand we need to reverse complement the genomic sequence and the mismatch_call
	    if ($strand eq '-'){
	      $genome_1_sequence = reverse_complement($genome_1_sequence);
	      $mismatch_info_1 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_1);
	    }
	  }
	  elsif ($index == 1){
	    $genome_2_sequence = substr($genome_2{$chr},$start-1,length$sequence); # subtract -1 from the start position again
	    $mismatch_info_2 = $mismatch_info;
	
	    ### if the alignment came from the reverse strand we need to reverse complement the genomic sequence and the mismatch_call
	    if ($strand eq '-'){
	      $genome_2_sequence = reverse_complement($genome_2_sequence);
	      $mismatch_info_2 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_2);
	    }
	  }

	  ### determining the best alignment for the other genome (if there is one at all within the restrictions of the alignment parameters set)
	
       	  my $key_1;          # first alignment to the other genome
	  my $mm_1;	
	  my $alignment_1;
	
	  my $key_2;          # second alignment to the other genome
	  my $mm_2;
	  my $alignment_2;

	  foreach my $mm (sort {$a<=>$b} keys %mismatches){

	    next unless ($mm > $mismatch_number); # per definition the next best hit (if there is one) must have more mismatches than the unique best hit
	
	    foreach my $alignment_position (keys %{$mismatches{$mm}} ){

	      my $ind = $mismatches{$mm}->{$alignment_position}->{index};

	      next if ($ind == $index); ### this is the second hit to the same genome and not the first hit to the second genome

	      ### assigning the first alignment to the second genome
	      unless (defined $key_1){
		$key_1 = $alignment_position;
		$mm_1 = $mm;
		$alignment_1 = $mismatches{$mm}->{$alignment_position}->{line};
	      }
	      ### assigning the second alignment to the second genome if there was already a first one
	      else{
		$key_2 = $alignment_position;
		$mm_2 = $mm;
		$alignment_2 = $mismatches{$mm}->{$alignment_position}->{line};
	      }
	    }
	  }

	  ### Now looking for the best alignment to the second genome

	  if ($key_1){
	    ### there is at least 1 hit to the other genome:
	    # my ($chr_1,$pos_1,$index_1) = (split (/:/,$key_1));
	    my ($strand_1,$chr_1,$start_1,$bowtie_seq_1,$m_info_1) = (split (/\t/,$alignment_1),-1)[1,2,3,4,7];

	    $start_1 += 1; # bowtie alignments are 0 based
	
	    if ($index == 0){
	      $genome_2_sequence  = substr($genome_2{$chr_1},$start_1-1,length$sequence); # for the substring we need to subtract 1 again
	      $mismatch_info_2 = $m_info_1;
	
	      if ($strand_1 eq '-'){
		$genome_2_sequence = reverse_complement($genome_2_sequence);
		$mismatch_info_2 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_2);
	      }
	    }
	    elsif ($index == 1){
	      $genome_1_sequence = substr($genome_1{$chr_1},$start_1-1,length$sequence); # for the substring we need to subtract 1 again
	      $mismatch_info_1 = $m_info_1;

	      if ($strand_1 eq '-'){
		$genome_1_sequence = reverse_complement($genome_1_sequence);
		$mismatch_info_1 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_1);
	      }
	    }

	    if ($key_2){
	      ### there are 2 alignments to the other genome
	      #  my ($chr_2,$pos_2,$index_2) = (split (/:/,$key_2));
	      #  my ($bowtie_seq_2,$m_info_2) = (split (/\t/,$alignment_2))[4,7];

	
	      ### if both alignments to the second genome have the same number of mismatches we will enter 'MULT' into the sequence and mismatch fields (non-unique alignments)
	      if ($mm_1 == $mm_2){
		if ($index == 0){
		  $genome_2_sequence = 'MULT';
		  $mismatch_info_2 = 'MULT';
		}
		elsif ($index == 1){
		  $genome_1_sequence = 'MULT';
		  $mismatch_info_1 = 'MULT';
		}
	      }
	      elsif ($mm_1 < $mm_2){
		### alignment_1 is the best alignment to the second genome
		### we will just continue to use the other genome sequence and mismatch information as extracted above
	      }
	      else{
		die "mm_1 ($mm_1) cannot be higher than mm_2 ($mm_2)\n";
	      }
	    }

	    else{
	      ### there is only 1 hit to the second genome, which we will use to print out
	      ### we will just continue to use the other genome sequence and mismatch information as extracted above
	    }
	  }
	
	  ### if there is no hit to the other genome we will enter N/A into the sequence and mismatch field for the other genome
	  else{
	    if ($index == 0){
	      $genome_2_sequence = 'N/A';
	      $mismatch_info_2 = 'N/A';
	    }
	    elsif ($index == 1){
	      $genome_1_sequence = 'N/A';
	      $mismatch_info_1 = 'N/A';
	    }
	  }

	  ### Printing the read out (still within the --dissimilar option)
	
	  # read aligned uniquely best to genome 1
	  if ($index == 0){
	    $counting{genome_1_specific_count}++;
	    print OUT_G1 join ("\t",$id,$sequence,$index+1,$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2,$quality_value),"\n";
	    return 0; ## once we printed the sequence with the lowest number of mismatches we exit
	  }
	
	  # read aligned uniquely best to genome 2
	  elsif ($index == 1){
	    $counting{genome_2_specific_count}++;
	    print OUT_G2 join ("\t",$id,$sequence,$index+1,$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2,$quality_value),"\n";
	    return 0; ## once we printed the sequence with the lowest number of mismatches we exit
	  }
	  else{
	    die "\$index needs to be 1 or 2! (and was $index in this case....)\n";
	  }
       	}	
	### (b) if the genomes differ only in a number of SNP positions we are going to extract the corresponding sequence at the position of the alignment in the other genome,
	### and print this sequence as well as its mismatch information to the output file (DEFAULT)

	else{
	  my ($id,$strand,$chr,$start,$bowtie_sequence,$mismatch_info) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line},-1))[0,1,2,3,4,7];
	  $start += 1; # bowtie alignments are 0 based
	  my $end = $start+length($sequence)-1;

	  my $genome_1_sequence;
	  my $genome_2_sequence;
	  my $mismatch_info_1;
	  my $mismatch_info_2;
	
	  if ( length($genome_1{$chr}) >= $end){
	    $genome_1_sequence  = substr($genome_1{$chr},$start-1,length$sequence); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id chr: $chr, $start-$end\n";
	    return 1; # can print this to unmapped if desired
	  }
	
	  if ( length($genome_2{$chr}) >= $end){
	    $genome_2_sequence = substr($genome_2{$chr},$start-1,length$sequence); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id chr: $chr, $start-$end\n";
	    return 1; # can print this to unmapped if desired	
	  }
	


	
	  # read aligned uniquely best to genome 1
	  if ($index == 0){
	    $counting{genome_1_specific_count}++;
	    $mismatch_info_1 = $mismatch_info;
	    $mismatch_info_2 = '';  # we'll leave this field blank for the moment and let people figure the SNP out themselves if needed
	    ### reverse complementing sequences on the reverse strand so that they are directly comparable with the sequence in the supplied sequence file ($sequence)
	    if ($strand eq '-'){
	      $genome_1_sequence = reverse_complement($genome_1_sequence);
	      $genome_2_sequence = reverse_complement($genome_2_sequence);
	      $mismatch_info_1 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_1);
	    }

	    $mismatch_info_2 = determine_read_mismatches_to_genomic_sequence($sequence,$genome_2_sequence);

	    print OUT_G1 join ("\t",$id,$sequence,$index+1,$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2,$quality_value),"\n";
	    return 0; ## if we printed the sequence with the lowest number of mismatches we exit
	  }

	  # read aligned uniquely best to genome 2
	  elsif ($index == 1){
	    $counting{genome_2_specific_count}++;
	    $mismatch_info_1 = '';  # we'll leave this field blank for the moment and let people figure the SNP out themselves if needed
	    $mismatch_info_2 = $mismatch_info;

	    ### reverse complementing sequences on the reverse strand so that they are directly comparable with the sequence in the supplied sequence file ($sequence)
	    if ($strand eq '-'){
	      $genome_1_sequence = reverse_complement($genome_1_sequence);
	      $genome_2_sequence = reverse_complement($genome_2_sequence);
	      $mismatch_info_2 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_2);
	    }

	    $mismatch_info_1 = determine_read_mismatches_to_genomic_sequence($sequence,$genome_1_sequence);
	
	    print OUT_G2 join ("\t",$id,$sequence,$index+1,$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2,$quality_value),"\n";
	    return 0; ## if we printed the sequence with the lowest number of mismatches we exit
	  }

	  else{
	    die "Index number was $index and can only be 0 or 1\n";
	  }
	}
      }
      ### as there is only one key in the hash with the lowest number of mismatches we do not neet to exit the foreach-loop
    }

    elsif (scalar keys %{$mismatches{$mismatch_number}} == 2){

      ### here we have to discriminate a few different cases:
      ### (a) both sequence alignments come from the same genome (= $index) => thus the sequence can't be mapped uniquely and needs to be discarded
      ### (b) the sequence aligns equally well to the two genomes, but to different locations: the sequence will be discarded
      ### (c) the sequence aligns equally well to the two different genomes => the sequence alignment will be printed as alignments in common (OUT_MIXED)

      my $key_1;
      my $alignment_1;
      my $key_2;
      my $alignment_2;

      foreach my $key (keys %{$mismatches{$mismatch_number}}){
	unless ($key_1){
	  $key_1 = $key;
	  $alignment_1 = $mismatches{$mismatch_number}->{$key}->{line};
	}
	else{
	  $key_2 = $key;
	  $alignment_2 = $mismatches{$mismatch_number}->{$key}->{line};
	}
      }

      my ($chr_1,$pos_1,$index_1) = (split (/:/,$key_1));
      my ($chr_2,$pos_2,$index_2) = (split (/:/,$key_2));

      if ($index_1 == $index_2){
	### this is (a), read is not uniquely mappable
	++$counting{ambiguous_mapping_count};
	return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
      }

      elsif ($chr_1 ne $chr_2 or $pos_1 != $pos_2){
	### this is (b), read will be chucked
	$counting{unsuitable_sequence_count}++;
	return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
      }

      elsif ($chr_1 eq $chr_2 and $pos_1 == $pos_2){

	### the concept of homologous sequences is not supported for --dissimilar genomes. Thus there will be no common alignments output
	if ($dissimilar){
	  $counting{equivalent_sequences_dissimilar_count}++;
	  return 1; ## can be printed out to unmapped.out if there is no unique match
	}
	++$counting{aligns_to_both_genomes_equally_well_count};
	
	### this is (c), we will print the read out to OUT_MIXED

	my ($id,$strand,$chr,$start,$bowtie_sequence,$mismatch_info_1) = (split (/\t/,$alignment_1,-1))[0,1,2,3,4,7];
	$start += 1;
	my $end = $start+length($sequence)-1;

	my ($mismatch_info_2) = (split (/\t/,$alignment_2,-1))[7];
	
	my $genome_1_sequence = substr($genome_1{$chr},$start-1,length$sequence);
	my $genome_2_sequence = substr($genome_2{$chr},$start-1,length$sequence);


	### reverse complementing sequences on the reverse strand so that they are directly comparable with the sequence in the supplied sequence file ($sequence)
	if ($strand eq '-'){
	  $genome_1_sequence = reverse_complement($genome_1_sequence);
	  $genome_2_sequence = reverse_complement($genome_2_sequence);
	  $mismatch_info_1 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_1);
	  $mismatch_info_2 = get_reverse_strand_mismatch_call ($sequence,$mismatch_info_2);
	}	

	print OUT_MIXED join ("\t",$id,$sequence,'N',$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2,$quality_value),"\n";
	# print join ("\t",$id,$sequence,'N',$strand,$chr,$start,$end,$genome_1_sequence,$mismatch_info_1,$genome_2_sequence,$mismatch_info_2),"\n";
      }
      else{
	die "Unexpected chr/pos/index combination\n\n";
      }
      return 0; ## the sequence must have been either returned or printed out, and we want to only process the lowest number of mismatches
    }

    elsif (scalar keys %{$mismatches{$mismatch_number}} == 3 or scalar keys %{$mismatches{$mismatch_number}} == 4 ){
      ### in any case, if there are 3 or 4 alignment positions with the same number of lowest mismatches for a given sequence we can't map it uniquely and discard the sequence
      ++$counting{ambiguous_mapping_count};
      return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
    }

    else{
      die "Unexpected number of elements with a lowest number of mismatches: ",scalar keys %{$mismatches{$mismatch_number}},"\n";
    }

    last; # unless we exited already we will exit the loop after we processed the sequence with the lowest number of mismatches

  }

  return 0; # sequence will not get printed to unmapped.out (1 would have been returned at a previous point already)

}

sub get_reverse_strand_mismatch_call{
  my $seq = shift;
  my $old_mm_call = shift;

  if ($old_mm_call eq ''){
    return $old_mm_call;
  }
  else{
    die "The mismatch field looks like this: $old_mm_call and can't be reverse complemented\n" unless ($old_mm_call =~ /^\d+/);

    my @old_mismatches = split (/,/,$old_mm_call);
    my @new_mismatches;

    foreach my $mm (@old_mismatches){
      my ($offset,$bases) = (split /:/,$mm); # the offset is based on the read, so it stays the same
      my ($ref_base,$read_base) = (split />/,$bases);

      unless ( (length $ref_base == 1) and (length $read_base == 1) ){
	die "The reference ($ref_base) and read base ($read_base) fields caused an error\n";
      }

      ### generating mismatch info for the reverse strand
      #   my $reverse_offset =  (length($seq))-1-$offset; # the offsets are 0 based, the read length is thus 1 bp longer

      my $reverse_ref_base = $ref_base;
      $reverse_ref_base =~ tr/GATC/CTAG/;
      my $reverse_read_base = $read_base;
      $reverse_read_base =~ tr/GATC/CTAG/;

      #  print "$offset\t$ref_base\t$read_base\n$reverse_offset\t$reverse_ref_base\t$reverse_read_base\n\n";
      my $reverse_mm = "${offset}:${reverse_ref_base}>${reverse_read_base}";

      ### we are adding the mismatches to the beginning of the array so they are in the correct order when the sequence is displayed as its reverse complement
      push @new_mismatches, $reverse_mm;
    }

    my $new_mm_call = join (",",@new_mismatches);
    return $new_mm_call;
  }
}


sub determine_read_mismatches_to_genomic_sequence{
  my ($seq,$seq_2) = @_; ## seq_2 is the genomic sequence of the genome which had more mismatches than $seq to the best alignment genome

  my @seq = split //,$seq;
  my @seq_2 = split //,$seq_2;

  unless (scalar@seq == scalar@seq_2){
    die "Sequences $seq and $seq_2 are not of the same length!\n\n";
  }
  my @mismatches;

  foreach my $index (0..$#seq){
    unless ($seq[$index] eq $seq_2[$index]){
      my $mm = "$index:$seq_2[$index]>$seq[$index]"; ## $seq_2[$index] is the reference genome base, $seq[$index] is the read base
      push @mismatches,$mm;
    }
  }
  my $mismatch_call = join (',',@mismatches);
  return $mismatch_call;
}




#######################################################################################################################################
### Checking bowtie results (paired-end)


sub check_bowtie_results_paired_ends{

  my ($sequence_1,$sequence_2,$identifier,$quality_value_1,$quality_value_2) = @_;
  my %mismatches = ();

  unless ($quality_value_1 and $quality_value_2){ ### quality values are not given for FastA files, so they are initialised as empty strings
    $quality_value_1 = $quality_value_2 = '';
  }

  ### we will output the FastQ quality in Sanger encoding (Phred 33 scale)
  if ($phred64){
    $quality_value_1 = convert_phred64_quals_to_phred33($quality_value_1);
    $quality_value_2 = convert_phred64_quals_to_phred33($quality_value_2);
  }
  elsif ($solexa){
    $quality_value_1 = convert_solexa_quals_to_phred33($quality_value_1);
    $quality_value_2 = convert_solexa_quals_to_phred33($quality_value_2);
  }

  ### reading from the bowtie output filehandles to see if this sequence pair aligned to one of the two genomes
  foreach my $index (0..$#fhs){

    ### skipping this index if the last alignment has been set to undefined already (i.e. end of bowtie output)
    ### last line_1 and _2 will store both sides of the paired-end alignments
    next unless ($fhs[$index]->{last_line_1} and $fhs[$index]->{last_line_2} and $fhs[$index]->{last_seq_id});

    ### if the sequence pair we are currently looking at produced an alignment we are doing various things with it
    if ($fhs[$index]->{last_seq_id} eq $identifier) {

      ###############################################################
      ### STEP I Now processing the alignment stored in last_line ###
      ###############################################################

      ### chomping the alignments now so we don't have to do it multiple times
      chomp $fhs[$index]->{last_line_1};
      chomp $fhs[$index]->{last_line_2};

      ### extract some useful information from the Bowtie output
      my ($id_1,$strand_1,$mapped_chromosome_1,$position_1,$bowtie_sequence_1,$mismatch_info_1) = (split (/\t/,$fhs[$index]->{last_line_1},-1))[0,1,2,3,4,7];
      my ($id_2,$strand_2,$mapped_chromosome_2,$position_2,$bowtie_sequence_2,$mismatch_info_2) = (split (/\t/,$fhs[$index]->{last_line_2},-1))[0,1,2,3,4,7];

      ### Now extracting the number of mismatches
      my $number_of_mismatches_1;
      my $number_of_mismatches_2;

      if ($mismatch_info_1 eq ''){
	$number_of_mismatches_1 = 0;
      }
      elsif ($mismatch_info_1 =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info_1);
	$number_of_mismatches_1 = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field:\t>>> $mismatch_info_1 <<<\n";
      }

      if ($mismatch_info_2 eq ''){
	$number_of_mismatches_2 = 0;
      }
      elsif ($mismatch_info_2 =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info_2);
	$number_of_mismatches_2 = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field:\t>>> $mismatch_info_2 <<<\n";
      }

      ### To decide whether a sequence pair has a unique best alignment we will look at the lowest sum of mismatches from both alignments
      my $sum_of_mismatches = $number_of_mismatches_1+$number_of_mismatches_2;

      ### creating a composite location variable from $chromosome and $position and storing the alignment information in a temporary hash table
      die "Position 1 ($position_1) is greater than position 2 ($position_2)\n" if ($position_1 > $position_2);
      die "Paired-end alignments need to be on the same chromosome\n" unless ($mapped_chromosome_1 eq $mapped_chromosome_2);

      ### creating a composite location variable from $mapped_chromosome, $position and $index and storing the alignment information in a temporary hash table
      my $alignment_location = join (":",$mapped_chromosome_1,$position_1,$position_2,$index);

      $mismatches{$sum_of_mismatches}->{$alignment_location}->{line_1} = $fhs[$index]->{last_line_1};
      $mismatches{$sum_of_mismatches}->{$alignment_location}->{line_2} = $fhs[$index]->{last_line_2};
      $mismatches{$sum_of_mismatches}->{$alignment_location}->{index} = $index;


      ###################################################################################################################################################
      ### STEP II Now reading in the next 2 lines from the bowtie filehandle. If there are 2 next lines in the alignments filehandle it can either    ###
      ### be a second alignment of the same sequence pair or a new sequence pair. In any case we will just add it to last_line_1 and last_line _2     ###
      ###################################################################################################################################################

      my $newline_1 = $fhs[$index]->{fh}-> getline();
      my $newline_2 = $fhs[$index]->{fh}-> getline();

      if ($newline_1 and $newline_2){
	chomp $newline_1;
	chomp $newline_2;
	my ($seq_id_1) = split (/\t/,$newline_1);
	my ($seq_id_2) = split (/\t/,$newline_2);
	$seq_id_1 =~ s/\/[12]//; # removing the read 1 or read 2 tag
	$seq_id_2 =~ s/\/[12]//; # removing the read 1 or read 2 tag
	die "Seq IDs need to be identical!\nID1:\t$seq_id_1\nID2:\t$seq_id_2\n" unless ($seq_id_1 eq $seq_id_2);
	$fhs[$index]->{last_seq_id} = $seq_id_1; # either is fine
	$fhs[$index]->{last_line_1} = $newline_1;
	$fhs[$index]->{last_line_2} = $newline_2;
      }
      else {
	# assigning undef to last_seq_id and both last_lines and jumping to the next index (end of bowtie output)
	$fhs[$index]->{last_seq_id} = undef;
	$fhs[$index]->{last_line_1} = undef;
	$fhs[$index]->{last_line_2} = undef;
	next; # jumping to the next index
      }

      ### If the new alignment is already the next entry we will process it further in the next round only.
      next unless ($fhs[$index]->{last_seq_id} eq $identifier);

      ### If it is a second alignment for the same ID we will continue adding it to the %mismatches hash

      ### reading the second reported alignment for a sequence. Resetting the variables to ensure they are fresh
      $id_1 = $strand_1 = $mapped_chromosome_1 = $position_1 = $bowtie_sequence_1 = $mismatch_info_1 = undef;
      $id_2 = $strand_2 = $mapped_chromosome_2 = $position_2 = $bowtie_sequence_2 = $mismatch_info_2 = undef;

      ($id_1,$strand_1,$mapped_chromosome_1,$position_1,$bowtie_sequence_1,$mismatch_info_1) = (split (/\t/,$fhs[$index]->{last_line_1},-1))[0,1,2,3,4,7];
      ($id_2,$strand_2,$mapped_chromosome_2,$position_2,$bowtie_sequence_2,$mismatch_info_2) = (split (/\t/,$fhs[$index]->{last_line_2},-1))[0,1,2,3,4,7];

      ### Now extracting the number of mismatches for the first sequence
      $number_of_mismatches_1 = undef;
      $number_of_mismatches_2 = undef;

      if ($mismatch_info_1 eq ''){
	$number_of_mismatches_1 = 0;
      }
      elsif ($mismatch_info_1 =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info_1);
	$number_of_mismatches_1 = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field:\t >>> $mismatch_info_1 <<<\n";
      }

      if ($mismatch_info_2 eq ''){
	$number_of_mismatches_2 = 0;
      }
      elsif ($mismatch_info_2 =~ /^\d+/){
	my @mismatches = split (/,/,$mismatch_info_2);
	$number_of_mismatches_2 = scalar @mismatches;
      }
      else{
	die "Something weird is going on with the mismatch field:\t >>> $mismatch_info_2 <<<\n";
      }

      ### To decide whether a sequence pair has a unique best alignment we will look at the lowest sum of mismatches from both alignments
      $sum_of_mismatches = $number_of_mismatches_1+$number_of_mismatches_2;

      die "Position 1 ($position_1) is greater than position 2 ($position_2)\n" if ($position_1 > $position_2);
      die "Paired-end alignments need to be on the same chromosome\n" unless ($mapped_chromosome_1 eq $mapped_chromosome_2);

      ### creating a composite location variable from $mapped_chromosome_1, $position_1, $position_2 and $index and storing the alignment information in a temporary hash table
      $alignment_location = undef;
      $alignment_location = join (":",$mapped_chromosome_1,$position_1,$position_2,$index);

      $mismatches{$sum_of_mismatches}->{$alignment_location}->{line_1} = $fhs[$index]->{last_line_1};
      $mismatches{$sum_of_mismatches}->{$alignment_location}->{line_2} = $fhs[$index]->{last_line_2};
      $mismatches{$sum_of_mismatches}->{$alignment_location}->{index} = $index;

      ###############################################################################################################################################
      ### STEP III Now reading in two more lines. These have to be the next entry and we will just add assign them to last_line_1 and last_line_2 ###
      ###############################################################################################################################################

      $newline_1 = $fhs[$index]->{fh}-> getline();
      $newline_2 = $fhs[$index]->{fh}-> getline();

      if ($newline_1 and $newline_2){
	chomp $newline_1;
	chomp $newline_2;
	my ($seq_id_1) = split (/\t/,$newline_1);
	my ($seq_id_2) = split (/\t/,$newline_2);
	$seq_id_1 =~ s/\/[12]//; # removing the read 1 or read 2 tag
	$seq_id_2 =~ s/\/[12]//; # removing the read 1 or read 2 tag
	die "Seq IDs need to be identical!\nID1:\t$seq_id_1\nID2:\t$seq_id_2\n" unless ($seq_id_1 eq $seq_id_2);
	$fhs[$index]->{last_seq_id} = $seq_id_1; # either is fine
	$fhs[$index]->{last_line_1} = $newline_1;
	$fhs[$index]->{last_line_2} = $newline_2;
      }
      else {
	# assigning undef to last_seq_id and both last_lines and jumping to the next index (end of bowtie output)
	$fhs[$index]->{last_seq_id} = undef;
	$fhs[$index]->{last_line_1} = undef;
	$fhs[$index]->{last_line_2} = undef;
	next; # jumping to the next index
      }
    } ### still within the foreach index loop
  }

  ### if there was no alignment for a certain sequence we will continue with the next sequence in the sequence file
  unless(%mismatches){
    $counting{no_single_alignment_found}++;
    return 1; ### We will print this sequence out as unmapped sequence if --un unmapped.out has been specified
  }

  #######################################################################################################################################################
  ### We are now looking if there is a unique best alignment for a certain sequence. This means we are sorting in ascending order and looking at the  ###
  ### sequence with the lowest amount of mismatches.                                                                                                  ###
  #######################################################################################################################################################

  ### sort in ascending order
  for my $mismatch_number (sort {$a<=>$b} keys %mismatches){

    ### if there is only 1 entry in the hash with the lowest number of mismatches the sequence is unique to one of the genomes
    if (scalar keys %{$mismatches{$mismatch_number}} == 1){

      ### unique best alignment here is in fact the composite chromosome:position_1:position_2:index string
      for my $unique_best_alignment (keys %{$mismatches{$mismatch_number}}){

       	### we neeed to discriminate the following 2 cases:
	### (a) genomes are dissimilar (e.g. one genome is only a single chromosome of another species). This needs to be specified with the --dissimilar option.
	### (b) both genomes are essentially the same and differ only in a number of SNPs. This is the default option
	
	my $index = $mismatches{$mismatch_number}->{$unique_best_alignment}->{index};

	### (a) if the genomes are dissimilar we are going to write out the genome-specific alignment and it's coordinates and will also write out the
	### best alignment to the other genome and its mismatch information

 	if ($dissimilar){
	  my ($id_1,$strand_1,$chr_1,$start_1,$bowtie_sequence_1,$mismatch_info_1) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line_1},-1))[0,1,2,3,4,7];
	  my ($id_2,$strand_2,$chr_2,$start_2,$bowtie_sequence_2,$mismatch_info_2) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line_2},-1))[0,1,2,3,4,7];

	  $start_1 += 1; # bowtie alignments are 0 based
	  $start_2 += 1;

	  my $end_1;
	  my $end_2;

	  if ($id_1 =~ /\/1$/){  # $sequence_1 aligned to the forward strand
	    $end_1 = $start_1+length($sequence_1)-1;
	    $end_2 = $start_2+length($sequence_2)-1;
	  }
	  elsif ($id_1 =~ /\/2$/){  # $sequence_2 aligned to the forward strand
	    $end_1 = $start_1+length($sequence_2)-1;
	    $end_2 = $start_2+length($sequence_1)-1;
	  }
	
	  my ($genome_1_sequence_1,$genome_1_sequence_2,$genome_2_sequence_1,$genome_2_sequence_2,$mismatch_info_1_1,$mismatch_info_1_2,$mismatch_info_2_1,$mismatch_info_2_2);
	
	  # extracting the genomic sequences and mismatch information of the best alignment
	  if ($index == 0){

	    # sequence pair aligned uniquely best to genome 1

	    $mismatch_info_1_1 = $mismatch_info_1;
	    $mismatch_info_1_2 = $mismatch_info_2;
	
	    ### reverse complementing the mismatch info on the reverse strand (the second read) so that they are directly comparable with the sequence
	    $mismatch_info_1_2 = get_reverse_strand_mismatch_call ($sequence_2,$mismatch_info_1_2);

	    if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	      if ( length($genome_1{$chr_1}) >= $end_1){
		$genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
		return 1; # can print this to unmapped if desired
	      }
	
	      if ( length($genome_1{$chr_2}) >= $end_2){
		$genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		### reverse complementing this sequence as the second pair is always on the reverse strand
		$genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
		return 1; # can print this to unmapped if desired
	      }
	    }
	
	    else{ # sequence_2 aligned to the forward strand
	      if ( length($genome_1{$chr_1}) >= $end_1){
		$genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
		return 1; # can print this to unmapped if desired
	      }
	
	      if ( length($genome_1{$chr_2}) >= $end_2){
		$genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		### reverse complementing this sequence as the second pair is always on the reverse strand
		$genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
		return 1; # can print this to unmapped if desired
	      }
	    }
	  }

	  elsif ($index == 1){
	    # sequence pair aligned uniquely best to genome 2
	    $mismatch_info_2_1 = $mismatch_info_1;
	    $mismatch_info_2_2 = $mismatch_info_2;
	
	    ### reverse complementing the mismatch info on the reverse strand (the second read) so that they are directly comparable with the sequence
	    $mismatch_info_2_2 = get_reverse_strand_mismatch_call ($sequence_2,$mismatch_info_2_2);
	
	    if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	      if ( length($genome_2{$chr_1}) >= $end_1){
		$genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
		return 1; # can print this to unmapped if desired
	      }
	
	      if ( length($genome_2{$chr_2}) >= $end_2){
		$genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		### reverse complementing this sequence as the second pair is always on the reverse strand
		$genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
		return 1; # can print this to unmapped if desired
	      }
	    }
	    else{ # sequence_2 aligned to the forward strand
	      if ( length($genome_2{$chr_1}) >= $end_1){
		$genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
		return 1; # can print this to unmapped if desired
	      }
	
	      if ( length($genome_2{$chr_2}) >= $end_2){
		$genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		### reverse complementing this sequence as the second pair is always on the reverse strand
		$genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	      }
	      else{
		++$counting{unable_to_extract_genomic_sequence_count};
		warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
		return 1; # can print this to unmapped if desired
	      }
	    }	
	  }

 	  ### determining the best alignment to the other genome (if there is one at all within the restrictions of the alignment parameters set)
	
	  my $key_1;          # first alignment to the other genome
 	  my $mm_1;	
 	  my $alignment_1_1;
	  my $alignment_1_2;

 	  my $key_2;          # second alignment to the other genome
 	  my $mm_2;

 	  foreach my $mm (sort {$a<=>$b} keys %mismatches){

 	    next unless ($mm > $mismatch_number); # per definition the next best hit (if there is one) must have more mismatches than the unique best hit
	
 	    foreach my $alignment_position (keys %{$mismatches{$mm}} ){

 	      my $ind = $mismatches{$mm}->{$alignment_position}->{index};

 	      next if ($ind == $index); ### this is the second hit to the same genome and not the first hit to the second genome

 	      ### assigning the first alignment to the second genome
 	      unless (defined $key_1){
 		$key_1 = $alignment_position;
 		$mm_1 = $mm;
 		$alignment_1_1 = $mismatches{$mm}->{$alignment_position}->{line_1};
		$alignment_1_2 = $mismatches{$mm}->{$alignment_position}->{line_2};
 	      }
 	      ### assigning the second alignment to the second genome if there was already a first one
 	      else{
 		$key_2 = $alignment_position;
 		$mm_2 = $mm;
 		# $alignment_2_1 = $mismatches{$mm}->{$alignment_position}->{line_1}; # not needed for further analysis
 	   	# $alignment_2_2 = $mismatches{$mm}->{$alignment_position}->{line_2}; # not needed for further analysis
 	      }
 	    }
 	  }

 	  ### Now looking for the best alignment to the second genome

 	  if ($key_1){
	    ### there is at least 1 hit to the other genome:
	
 	    if ($key_2){
 	      ### there are 2 alignments to the other genome
	      #  my ($chr_2,$pos_2_1,$pos_2_2,$index_2) = (split (/:/,$key_2));
	      #  my ($bowtie_seq_2,$m_info_2) = (split (/\t/,$alignment_2))[4,7];
	
 	      ### if both alignments to the second genome have the same number of mismatches we will enter 'MULT' into the sequence and mismatch fields (non-unique alignments)
 	      if ($mm_1 == $mm_2){
 		if ($index == 0){
 		  $genome_2_sequence_1 = 'MULT';
 		  $mismatch_info_2_1   = 'MULT';
		  $genome_2_sequence_2 = 'MULT';
 		  $mismatch_info_2_2   = 'MULT';
 		}
		elsif ($index == 1){
 		  $genome_1_sequence_1 = 'MULT';
 		  $mismatch_info_1_1   = 'MULT';
		  $genome_1_sequence_2 = 'MULT';
 		  $mismatch_info_1_2   = 'MULT';
 		}
 	      }

 	      elsif ($mm_1 < $mm_2){
 		### alignment_1 is the best alignment to the second genome
 		### we will just continue to extract the other genome sequence and mismatch information
	      }
	      else{
 		die "mm_1 ($mm_1) cannot be higher than mm_2 ($mm_2)\n";
 	      }
 	    }

 	    else{
 	      ### there is only 1 hit to the second genome, which we will use to print out
 	      ### we will just continue to extract the other genome sequence and mismatch information
 	    }


	    my ($id_key_2_1,$strand_2_1,$chr_2_1,$start_2_1,$m_info_1) = (split (/\t/,$alignment_1_1),-1)[0,1,2,3,7];
	    my ($id_key_2_2,$strand_2_2,$chr_2_2,$start_2_2,$m_info_2) = (split (/\t/,$alignment_1_2),-1)[0,1,2,3,7];

 	    $start_2_1 += 1; # bowtie alignments are 0 based
	    $start_2_2 += 1;

	    ### we are going to use display the alignment in the same way the unique best alignment is presented
	    ### id_1 and id_2 have the information about which read mapped to which strand in the best alignment to the other genome

	    if ($index == 0){ # the unique best alignment aligns to the first genome

	      if ($id_1 =~ /\/1$/){ # sequence_1 of the unique hit aligned to the forward strand
		
		if ($id_key_2_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand of the other genome
		  $genome_2_sequence_1  = substr($genome_2{$chr_2_1},$start_2_1-1,length$sequence_1); # for the substring we need to subtract 1 again

		  $genome_2_sequence_2  = substr($genome_2{$chr_2_2},$start_2_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence as the second pair is always on the reverse strand
		  $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);

		  $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_1);
		  $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_2);	
		
		}

		elsif ($id_key_2_1 =~ /\/2$/){ # sequence_2 aligned to the forward strand
		  $genome_2_sequence_1  = substr($genome_2{$chr_2_2},$start_2_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence so we can compare the same sequences
		  $genome_2_sequence_1 = reverse_complement($genome_2_sequence_1);

		  $genome_2_sequence_2  = substr($genome_2{$chr_2_1},$start_2_1-1,length$sequence_2); # for the substring we need to subtract 1 again

		  $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_1);
		  $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_2);	

		}
		else{
		  die "The alignment to the other genome must end with /1 or /2\n";
		}
	
	      }
	
	      else{ # sequence_2 of the unique hit aligned to the forward strand

		if ($id_key_2_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand of the other genome

		  $genome_2_sequence_1  = substr($genome_2{$chr_2_2},$start_2_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence
		  $genome_2_sequence_1 = reverse_complement($genome_2_sequence_1);
		
		  $genome_2_sequence_2  = substr($genome_2{$chr_2_1},$start_2_1-1,length$sequence_1); # for the substring we need to subtract 1 again
		
		  $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_1);
		  $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_2);	

		}
		
		elsif ($id_key_2_1 =~ /\/2$/){ # sequence_2 aligned to the forward strand
		  $genome_2_sequence_1  = substr($genome_2{$chr_2_1},$start_2_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	
		  $genome_2_sequence_2  = substr($genome_2{$chr_2_2},$start_2_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence so we can compare the same sequences
		  $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);

		  $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_1);
		  $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_2);	

		}
		else{
		  die "The alignment to the other genome must end with /1 or /2\n";
		}
	      }
	    }
	
	    elsif ($index == 1){ # the unique best alignment aligns to the second genome
	
	      if ($id_1 =~ /\/1$/){ # sequence_1 of the unique hit aligned to the forward strand

		if ($id_key_2_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand of the other genome
		  $genome_1_sequence_1  = substr($genome_1{$chr_2_1},$start_2_1-1,length$sequence_1); # for the substring we need to subtract 1 again

		  $genome_1_sequence_2  = substr($genome_1{$chr_2_2},$start_2_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence so we can compare the same sequences in each field
		  $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
		
		  $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_1);
		  $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_2);		
		
		}
		
		elsif ($id_key_2_1 =~ /\/2$/){ # sequence_2 aligned to the forward strand
		  $genome_1_sequence_1  = substr($genome_1{$chr_2_2},$start_2_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence so we can compare the same sequences in each field
		  $genome_1_sequence_1 = reverse_complement($genome_1_sequence_1);
		
		  $genome_1_sequence_2  = substr($genome_1{$chr_2_1},$start_2_1-1,length$sequence_2); # for the substring we need to subtract 1 again
		
		  $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_1);
		  $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_2);		
		
		}
		else{
		  die "The alignment to the other genome must end with /1 or /2\n";
		}
	      }

	      else{ # sequence_2 of the unique hit aligned to the forward strand

		if ($id_key_2_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand of the other genome

		  $genome_1_sequence_1  = substr($genome_1{$chr_2_2},$start_2_2-1,length$sequence_2); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence
		  $genome_1_sequence_1 = reverse_complement($genome_1_sequence_1);
		
		  $genome_1_sequence_2  = substr($genome_1{$chr_2_1},$start_2_1-1,length$sequence_1); # for the substring we need to subtract 1 again
		
		  $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_1);
		  $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_2);		
		
		}
		
		elsif ($id_key_2_1 =~ /\/2$/){ # sequence_2 aligned to the forward strand
		  $genome_1_sequence_1  = substr($genome_1{$chr_2_1},$start_2_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	
		  $genome_1_sequence_2  = substr($genome_1{$chr_2_2},$start_2_2-1,length$sequence_1); # for the substring we need to subtract 1 again
		  ### reverse complementing this sequence so we can compare the same sequences
		  $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
		
		  $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_1);
		  $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_2);		
		
		}
		else{
		  die "The alignment to the other genome must end with /1 or /2\n";
		}
	      }
	    }
	  }
	
 	  ### if there is no hit to the other genome we will enter N/A into the sequence and mismatch field for the other genome
 	  else{
 	    if ($index == 0){
 	      $genome_2_sequence_1 = 'N/A';
 	      $mismatch_info_2_1   = 'N/A';
	      $genome_2_sequence_2 = 'N/A';
 	      $mismatch_info_2_2   = 'N/A';
	    }
 	    elsif ($index == 1){
 	      $genome_1_sequence_1 = 'N/A';
 	      $mismatch_info_1_1   = 'N/A';
 	      $genome_1_sequence_2 = 'N/A';
 	      $mismatch_info_1_2   = 'N/A';
 	    }
 	  }

 	  ### Printing the read out (still within the --dissimilar option)
	
	  # read aligned uniquely best to genome 1
 	  if ($index == 0){
 	    $counting{genome_1_specific_count}++;

	    if ($id_1 =~ /\/1$/){ # sequence_1 aligned to the forward strand
	
	      ### print out sequence_1 first
	      print OUT_G1 join ("\t",$identifier,$sequence_1,$sequence_2,$index+1,$strand_1,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }
	
	    elsif ($id_1 =~ /\/2$/){ # $sequence_2 aligned to the forward strand

	      ### print out sequence_2 first  ## strand info = second read!
	      print OUT_G1 join ("\t",$identifier,$sequence_2,$sequence_1,$index+1,$strand_2,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }

 	    return 0; ## once we printed the sequence with the lowest sum of mismatches we exit
 	  }
	
 	  # read aligned uniquely best to genome 2
 	  elsif ($index == 1){
 	    $counting{genome_2_specific_count}++;

	    if ($id_1 =~ /\/1$/){ # sequence_1 aligned to the forward strand
	
	      ### print out sequence_1 first
	      print OUT_G2 join ("\t",$identifier,$sequence_1,$sequence_2,$index+1,$strand_1,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }

	    elsif ($id_1 =~ /\/2$/){ # $sequence_2 aligned to the forward strand

	      ### print out sequence_2 first ## strand info = second read!
	      print OUT_G2 join ("\t",$identifier,$sequence_2,$sequence_1,$index+1,$strand_2,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }
	
	    return 0; ## once we printed the sequence pair with the lowest sum of mismatches we exit
 	  }
 	  else{
	    die "\$index needs to be 1 or 2! (and was $index in this case....)\n";
 	  }
	}
	

	### (b) if the genomes differ only in a number of SNP positions we are going to extract the corresponding sequences at the position of the alignment in the other genome,
	### and print these sequences as well as their mismatch information to the output file (DEFAULT)

      else{
	  my ($id_1,$strand_1,$chr_1,$start_1,$mismatch_info_1) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line_1},-1))[0,1,2,3,7];
	  my ($id_2,$strand_2,$chr_2,$start_2,$mismatch_info_2) = (split (/\t/,$mismatches{$mismatch_number}->{$unique_best_alignment}->{line_2},-1))[0,1,2,3,7];

	  $start_1 += 1; # bowtie alignments are 0 based
	  $start_2 += 1;

	  my $end_1;
	  my $end_2;

	  if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	    $end_1 = $start_1+length($sequence_1)-1;
	    $end_2 = $start_2+length($sequence_2)-1;
	  }
	  elsif ($id_1 =~ /\/2$/){  # sequence_2 aligned to the forward strand
	    $end_1 = $start_1+length($sequence_2)-1;
	    $end_2 = $start_2+length($sequence_1)-1;
	  }
	
	  my ($genome_1_sequence_1,$genome_1_sequence_2,$genome_2_sequence_1,$genome_2_sequence_2,$mismatch_info_1_1,$mismatch_info_1_2,$mismatch_info_2_1,$mismatch_info_2_2);
	
	  # extracting all genomic sequences
	  if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	    if ( length($genome_1{$chr_1}) >= $end_1){
	      $genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	      return 1; # can print this to unmapped if desired
	    }
	
	    if ( length($genome_1{$chr_2}) >= $end_2){
	      $genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
	      ### reverse complementing this sequence as the second pair is always on the reverse strand
	      $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	      return 1; # can print this to unmapped if desired
	    }

	    if ( length($genome_2{$chr_1}) >= $end_1){
	      $genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	      return 1; # can print this to unmapped if desired
	    }
	
	    if ( length($genome_2{$chr_2}) >= $end_2){
	      $genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
	      ### reverse complementing this sequence as the second pair is always on the reverse strand
	      $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	      return 1; # can print this to unmapped if desired
	    }
	  }

	  else{ # sequence_2 aligned to the forward strand
	    if ( length($genome_1{$chr_1}) >= $end_1){
	      $genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	      return 1; # can print this to unmapped if desired
	    }
	
	    if ( length($genome_1{$chr_2}) >= $end_2){
	      $genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
	      ### reverse complementing this sequence as the second pair is always on the reverse strand
	      $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	      return 1; # can print this to unmapped if desired
	    }

	    if ( length($genome_2{$chr_1}) >= $end_1){
	      $genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	      return 1; # can print this to unmapped if desired
	    }
	
	    if ( length($genome_2{$chr_2}) >= $end_2){
	      $genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
	      ### reverse complementing this sequence as the second pair is always on the reverse strand
	      $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	    }
	    else{
	      ++$counting{unable_to_extract_genomic_sequence_count};
	      warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	      return 1; # can print this to unmapped if desired
	    }
	  }
	
	  # read aligned uniquely best to genome 1
	  if ($index == 0){
	    $counting{genome_1_specific_count}++;

	    $mismatch_info_1_1 = $mismatch_info_1;
	    $mismatch_info_1_2 = $mismatch_info_2;
		
	    ### now determining mismatch info for the other genome
	    if ($id_1 =~ /\/1$/){ # sequence_1 aligned to the forward strand
	
	      ### reverse complementing the mismatch info on the reverse strand (the second read) so that they are directly comparable with the sequence
	      $mismatch_info_1_2 = get_reverse_strand_mismatch_call ($sequence_2,$mismatch_info_1_2);

	      $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_1);
	      $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_2);
	
	      ### print out sequence_1 first
	      print OUT_G1 join ("\t",$identifier,$sequence_1,$sequence_2,$index+1,$strand_1,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }

  	    elsif ($id_1 =~ /\/2$/){ # $sequence_2 aligned to the forward strand
	
	      ### reverse complementing the mismatch info on the reverse strand (the second read) so that they are directly comparable with the sequence
	      $mismatch_info_1_2 = get_reverse_strand_mismatch_call ($sequence_1,$mismatch_info_1_2);

	      $mismatch_info_2_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_2_sequence_1);
	      $mismatch_info_2_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_2_sequence_2);

	      ### print out sequence_2 first ## strand info = second read!
	      print OUT_G1 join ("\t",$identifier,$sequence_2,$sequence_1,$index+1,$strand_2,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }
	
	    else{
	      die "The read identifiers are in an unexpected format and do not en on /1 and /2!\n";
	    }
	    return 0; ## if we printed the sequence with the lowest number of mismatches we exit
	  }

	  # read aligned uniquely best to genome 2
	  elsif ($index == 1){
	    $counting{genome_2_specific_count}++;

	    $mismatch_info_2_1 = $mismatch_info_1;
	    $mismatch_info_2_2 = $mismatch_info_2;
	
	    ### now determining mismatch info for the other genome
	    if ($id_1 =~ /\/1$/){ # sequence_1 aligned to the forward strand
	
	      ### reverse complementing the mismatch info on the reverse strand (always the second read) so that they are directly comparable with the sequence
	      $mismatch_info_2_2 = get_reverse_strand_mismatch_call ($sequence_2,$mismatch_info_2_2);

	      $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_1);
	      $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_2);
	
	      ### print out sequence_1 first
	      print OUT_G2 join ("\t",$identifier,$sequence_1,$sequence_2,$index+1,$strand_1,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }

	    elsif ($id_1 =~ /\/2$/){ # $sequence_2 aligned to the forward strand

	      ### reverse complementing the mismatch info on the reverse strand (always the second read) so that they are directly comparable with the sequence
	      $mismatch_info_2_2 = get_reverse_strand_mismatch_call ($sequence_1,$mismatch_info_2_2);

	      $mismatch_info_1_1 = determine_read_mismatches_to_genomic_sequence($sequence_2,$genome_1_sequence_1);
	      $mismatch_info_1_2 = determine_read_mismatches_to_genomic_sequence($sequence_1,$genome_1_sequence_2);

	      ### print out sequence_2 first ## strand infor = second read
	      print OUT_G2 join ("\t",$identifier,$sequence_2,$sequence_1,$index+1,$strand_2,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	    }

	    else{
	      die "The read identifiers are in an unexpected format and do not en on /1 and /2!\n";
	    }
	    ### OUT_G2
	
	    return 0; ## if we printed the sequence with the lowest number of mismatches we exit
	  }

	  else{
	    die "Index number was $index and can only be 0 or 1\n";
	  }
	}
      }
      ### as there is only one key in the hash with the lowest number of mismatches we do not neet to exit the foreach-loop
    }

    elsif (scalar keys %{$mismatches{$mismatch_number}} == 2){

      ### here we have to discriminate a several different cases:
      ### (a) both sequence alignments come from the same genome (= $index) => thus the sequence can't be mapped uniquely and needs to be discarded
      ### (b) the sequence pair aligns equally well to the two genomes, but to different locations: the sequence pair will be discarded
      ### (c) the sequence pair aligns equally well to the two different genomes => the sequence alignment will be printed as alignments in common (OUT_MIXED)

      my ($key_1,$alignment_1_1,$alignment_1_2,$key_2,$alignment_2_1,$alignment_2_2);

      foreach my $key (keys %{$mismatches{$mismatch_number}}){
	unless ($key_1){
 	  $key_1 = $key;
 	  $alignment_1_1 = $mismatches{$mismatch_number}->{$key}->{line_1};
	  $alignment_1_2 = $mismatches{$mismatch_number}->{$key}->{line_2};
 	}
 	else{
 	  $key_2 = $key;
 	  $alignment_2_1 = $mismatches{$mismatch_number}->{$key}->{line_1};
	  $alignment_2_2 = $mismatches{$mismatch_number}->{$key}->{line_2};
	}
      }

      my ($chr_1,$pos_1_1,$pos_1_2,$index_1) = (split (/:/,$key_1));
      my ($chr_2,$pos_2_1,$pos_2_2,$index_2) = (split (/:/,$key_2));

      if ($index_1 == $index_2){
 	### this is (a), read is not uniquely mappable
 	++$counting{ambiguous_mapping_count};
 	return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
      }

      elsif ($chr_1 ne $chr_2 or $pos_1_1 != $pos_2_1 or $pos_1_2 != $pos_2_2){
	### this is (b), read will be chucked
	$counting{unsuitable_sequence_count}++;
 	return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
      }

      elsif ($chr_1 eq $chr_2 and $pos_1_1 == $pos_2_1 and $pos_1_2 == $pos_2_2 ){

 	### the concept of homologous sequences is not supported for --dissimilar genomes. Thus there will be no common alignments output
 	if ($dissimilar){
 	  $counting{equivalent_sequences_dissimilar_count}++;
 	  return 1; ## can be printed out to unmapped.out if there is no unique match
 	}
	++$counting{aligns_to_both_genomes_equally_well_count};
	
 	### this is (c), we will print the read out to OUT_MIXED

	my ($id_1,$strand_1,$chrom_1,$start_1,$mismatch_info_1) = (split (/\t/,$alignment_1_1,-1))[0,1,2,3,7];
	my ($id_2,$strand_2,$chrom_2,$start_2,$mismatch_info_2) = (split (/\t/,$alignment_1_2,-1))[0,1,2,3,7];

	$start_1 += 1; # bowtie alignments are 0 based
	$start_2 += 1;

	my $end_1;
	my $end_2;

	if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	  $end_1 = $start_1+length($sequence_1)-1;
	  $end_2 = $start_2+length($sequence_2)-1;
	}
	elsif ($id_1 =~ /\/2$/){  # sequence_2 aligned to the forward strand
	  $end_1 = $start_1+length($sequence_2)-1;
	  $end_2 = $start_2+length($sequence_1)-1;
	}
	
	my ($genome_1_sequence_1,$genome_1_sequence_2,$genome_2_sequence_1,$genome_2_sequence_2,$mismatch_info_1_1,$mismatch_info_1_2,$mismatch_info_2_1,$mismatch_info_2_2);
	
	# extracting all genomic sequences
	if ($id_1 =~ /\/1$/){  # sequence_1 aligned to the forward strand
	  if ( length($genome_1{$chr_1}) >= $end_1){
	    $genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	    return 1; # can print this to unmapped if desired
	  }

	  if ( length($genome_1{$chr_2}) >= $end_2){
	    $genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
	    ### reverse complementing this sequence as the second pair is always on the reverse strand
	    $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	    return 1; # can print this to unmapped if desired
	  }
	
	  if ( length($genome_2{$chr_1}) >= $end_1){
	    $genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_1); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	    return 1; # can print this to unmapped if desired
	  }
	
	  if ( length($genome_2{$chr_2}) >= $end_2){
	    $genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_2); # for the substring we need to subtract 1 again
	    ### reverse complementing this sequence as the second pair is always on the reverse strand
	    $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	    return 1; # can print this to unmapped if desired
	  }
	}

	else{ # sequence_2 aligned to the forward strand
	  if ( length($genome_1{$chr_1}) >= $end_1){
	    $genome_1_sequence_1  = substr($genome_1{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	    return 1; # can print this to unmapped if desired
	  }
	
	  if ( length($genome_1{$chr_2}) >= $end_2){
	    $genome_1_sequence_2  = substr($genome_1{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
	    ### reverse complementing this sequence as the second pair is always on the reverse strand
	    $genome_1_sequence_2 = reverse_complement($genome_1_sequence_2);
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	    return 1; # can print this to unmapped if desired
	  }

	  if ( length($genome_2{$chr_1}) >= $end_1){
	    $genome_2_sequence_1  = substr($genome_2{$chr_1},$start_1-1,length$sequence_2); # for the substring we need to subtract 1 again
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_1 chr: $chr_1, $start_1-$end_1\n";
	    return 1; # can print this to unmapped if desired
	  }
	
	  if ( length($genome_2{$chr_2}) >= $end_2){
	    $genome_2_sequence_2  = substr($genome_2{$chr_2},$start_2-1,length$sequence_1); # for the substring we need to subtract 1 again
	    ### reverse complementing this sequence as the second pair is always on the reverse strand
	    $genome_2_sequence_2 = reverse_complement($genome_2_sequence_2);
	  }
	  else{
	    ++$counting{unable_to_extract_genomic_sequence_count};
	    warn "Unable to extract genomic sequence for $id_2 chr: $chr_2, $start_2-$end_2\n";
	    return 1; # can print this to unmapped if desired
	  }
	}
	
	#### read alignment came from either genome, but as it has the same number of mismatches in the other genome and was found at the very same position
	### we will use the mismatch information for both genome 1 and genome 2
	
	### now determining mismatch info
	$mismatch_info_1_1 = $mismatch_info_2_1 = $mismatch_info_1;

	$mismatch_info_1_2 = $mismatch_info_2;	
	### reverse complementing the mismatch info on the reverse strand (the second read) so that they are directly comparable with the sequence
	$mismatch_info_1_2 = $mismatch_info_2_2 = get_reverse_strand_mismatch_call ($sequence_2,$mismatch_info_1_2);
	
	if ($id_1 =~ /\/1$/){ # sequence_1 aligned to the forward strand
	  ### printing out sequence_1 first
	  print OUT_MIXED join ("\t",$identifier,$sequence_1,$sequence_2,'N',$strand_1,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	}

	elsif ($id_1 =~ /\/2$/){ # $sequence_2 aligned to the forward strand
	  ### printing out sequence_2 first ## strand info = second read
	  print OUT_MIXED join ("\t",$identifier,$sequence_2,$sequence_1,'N',$strand_2,$chr_1,$start_1,$end_2,$genome_1_sequence_1,$mismatch_info_1_1,$genome_1_sequence_2,$mismatch_info_1_2,$genome_2_sequence_1,$mismatch_info_2_1,$genome_2_sequence_2,$mismatch_info_2_2,$quality_value_1,$quality_value_2),"\n";
	}
	
	else{
	  die "Unexpected chr/pos/index combination\n\n";
	}
      }
      return 0; ## the sequence pair must have been either returned or printed out, and we want to only process the lowest number of mismatches
    }

    elsif (scalar keys %{$mismatches{$mismatch_number}} == 3 or scalar keys %{$mismatches{$mismatch_number}} == 4 ){
      ### in any case, if there are 3 or 4 alignment positions with the same number of lowest mismatches for a given sequence we can't map it uniquely and discard the sequence
      ++$counting{ambiguous_mapping_count};
      return 1; # => exits to next sequence and prints it to unmapped.out if --un was specified
    }

    else{
      die "Unexpected number of elements with a lowest number of mismatches: ",scalar keys %{$mismatches{$mismatch_number}},"\n";
    }

    last; # we will exit the loop after we processed the sequence with the lowest number of mismatches

  }

  return 0; # sequence will not get printed to unmapped.out (1 would have been returned at a previous point already)

}

#######################################################################################################################################
### Firing up two instances of Bowtie (paired-end)


sub paired_end_align_fragments_fastA {
  my ($infile_1,$infile_2) = @_;
  print "Input files are $infile_1 and $infile_2 (FastA)\n\n";

  ## Now starting 2 instances of Bowtie feeding in the sequence file and aligning it to two different genomes. The first line of the bowtie output is read in and stored
  ## in the data structure @fhs
  warn "Now running 2 parallel instances of Bowtie against the two genomes\ngenome 1: $genome_index_basename_1\ngenome 2: $genome_index_basename_2\nspecified options: $bowtie_options\n\n";

  foreach my $fh (@fhs) {
    warn "Now starting a Bowtie paired-end alignment for $fh->{name} (reading in sequences from $fh->{inputfile_1} and $fh->{inputfile_2})\n";
    open ($fh->{fh},"$path_to_bowtie $bowtie_options $fh->{genome_index} -1 $fh->{inputfile_1} -2 $fh->{inputfile_2} |") or die "Can't open pipe to bowtie: $!";

    my $line_1 = $fh->{fh}->getline();
    my $line_2 = $fh->{fh}->getline();

    # if Bowtie produces an alignment we store the first line of the output
    if ($line_1 and $line_2) {
      my $id_1 = (split(/\t/),$line_1)[0]; # this is the first element of the first bowtie output line (= the sequence identifier)
      my $id_2 = (split(/\t/),$line_2)[0]; # this is the first element of the second bowtie output line
      $id_1 =~ s/\/[12]//; # removing the read 1 or read 2 tag
      $id_2 =~ s/\/[12]//; # removing the read 1 or read 2 tag
      if ($id_1 eq $id_2){
	$fh->{last_seq_id} = $id_1; # either will do
      }
      else {
	die "Sequence IDs do not match!\n"
      }
      $fh->{last_line_1} = $line_1; # this does contain the read 1 or read 2 tag
      $fh->{last_line_2} = $line_2; # this does contain the read 1 or read 2 tag
      warn "Found first alignment:\n$fh->{last_line_1}$fh->{last_line_2}";
    }
    # otherwise we just initialise last_seq_id and last_lines as undefined
    else {
      print "Found no alignment, assigning undef to last_seq_id and last_lines\n";
      $fh->{last_seq_id_1} = undef;
      $fh->{last_seq_id_2} = undef;
      $fh->{last_line_1} = undef;
      $fh->{last_line_2} = undef;
    }
  }
}

sub paired_end_align_fragments_fastQ {
  my ($infile_1,$infile_2) = @_;
  print "Input files are $infile_1 and $infile_2 (FastQ)\n\n";

  ## Now starting 2 instances of Bowtie feeding in the sequence file and aligning it to two different genomes. The first line of the bowtie output is read in and stored
  ## in the data structure @fhs
  warn "Now running 2 parallel instances of Bowtie against the two genomes\ngenome 1: $genome_index_basename_1\ngenome 2: $genome_index_basename_2\nspecified options: $bowtie_options\n\n";

  foreach my $fh (@fhs) {
    warn "Now starting a Bowtie paired-end alignment for $fh->{name} (reading in sequences from $fh->{inputfile_1} and $fh->{inputfile_2})\n";
    open ($fh->{fh},"$path_to_bowtie $bowtie_options $fh->{genome_index} -1 $fh->{inputfile_1} -2 $fh->{inputfile_2} |") or die "Can't open pipe to bowtie: $!";

    my $line_1 = $fh->{fh}->getline();
    my $line_2 = $fh->{fh}->getline();

    # if Bowtie produces an alignment we store the first line of the output
    if ($line_1 and $line_2) {
      my $id_1 = (split(/\t/,$line_1))[0]; # this is the first element of the first bowtie output line (= the sequence identifier)
      my $id_2 = (split(/\t/,$line_2))[0]; # this is the first element of the second bowtie output line
      $id_1 =~ s/\/[12]//; # removing the read 1 or read 2 tag
      $id_2 =~ s/\/[12]//; # removing the read 1 or read 2 tag
      if ($id_1 eq $id_2){
	$fh->{last_seq_id} = $id_1; # either will do
      }
      else {
	die "Sequence IDs do not match!\n"
      }
      $fh->{last_line_1} = $line_1; # this does contain the read 1 or read 2 tag
      $fh->{last_line_2} = $line_2; # this does contain the read 1 or read 2 tag
      warn "Found first alignment:\n$fh->{last_line_1}$fh->{last_line_2}";
    }
    # otherwise we just initialise last_seq_id and last_lines as undefined
    else {
      print "Found no alignment, assigning undef to last_seq_id and last_lines\n";
      $fh->{last_seq_id_1} = undef;
      $fh->{last_seq_id_2} = undef;
      $fh->{last_line_1} = undef;
      $fh->{last_line_2} = undef;
    }
  }
}



#######################################################################################################################################
### Firing up two instances of Bowtie (single-end)


sub single_end_align_fragments_fastA {
  my $infile = shift;
  print "Input file is $infile (FastA)\n\n";

  ## Now starting 2 instances of Bowtie feeding in the sequence file and aligning it to two different genomes. The first line of the bowtie output is read in and stored
  ## in the data structure @fhs
  warn "Now running 2 parallel instances of Bowtie against the two genomes\ngenome 1: $genome_index_basename_1\ngenome 2: $genome_index_basename_2\nspecified options: $bowtie_options\n\n";
  foreach my $fh (@fhs) {
    warn "Now starting Bowtie for $fh->{name} (reading in sequences from $fh->{inputfile})\n";
    open ($fh->{fh},"$path_to_bowtie $bowtie_options $fh->{genome_index} $fh->{inputfile} |") or die "Can't open pipe to bowtie: $!";

    # if Bowtie produces an alignment we store the first line of the output
    $_ = $fh->{fh}->getline();
    if ($_) {
      my $id = (split(/\t/))[0]; # this is the first element of the bowtie output (= the sequence identifier)
      $fh->{last_seq_id} = $id;
      $fh->{last_line} = $_;
      warn "Found first alignment:\t$fh->{last_line}\n";
    }
    # otherwise we just initialise last_seq_id and last_line as undefinded
    else {
      print "Found no alignment, assigning undef to last_seq_id and last_line\n";
      $fh->{last_seq_id} = undef;
      $fh->{last_line} = undef;
    }
  }
}

sub single_end_align_fragments_fastQ {
  my $infile = shift;
  print "Input file is $infile (FastQ)\n\n";

  ## Now starting 2 instances of Bowtie feeding in the sequence file and aligning it to two different genomes. The first line of the bowtie output is read in and stored
  ## in the data structure @fhs
  warn "Now running 2 parallel instances of Bowtie against the two genomes\ngenome 1: $genome_index_basename_1\ngenome 2: $genome_index_basename_2\nspecified options: $bowtie_options\n\n";
  foreach my $fh (@fhs) {
    warn "Now starting Bowtie for $fh->{name} (reading in sequences from $fh->{inputfile})\n";
    open ($fh->{fh},"$path_to_bowtie $bowtie_options $fh->{genome_index} $fh->{inputfile} |") or die "Can't open pipe to bowtie: $!";

    # if Bowtie produces an alignment we store the first line of the output
    $_ = $fh->{fh}->getline();
    if ($_) {
      my $id = (split(/\t/))[0]; # this is the first element of the bowtie output (= the sequence identifier)
      $fh->{last_seq_id} = $id;
      $fh->{last_line} = $_;
      warn "Found first alignment:\t$fh->{last_line}\n";
    }
    # otherwise we just initialise last_seq_id and last_line as undefined
    else {
      print "Found no alignment, assigning undef to last_seq_id and last_line\n";
      $fh->{last_seq_id} = undef;
      $fh->{last_line} = undef;
    }
  }
}

sub convert_phred64_quals_to_phred33{

  my $qual = shift;
  my @quals = split (//,$qual);
  my @new_quals;

  foreach my $index (0..$#quals){
    my $phred_score = convert_phred64_quality_string_into_phred_score ($quals[$index]);
    my $phred33_quality_string = convert_phred_score_into_phred33_quality_string ($phred_score);
    $new_quals[$index] = $phred33_quality_string;
  }

  my $phred33_quality = join ("",@new_quals);
  return $phred33_quality;
}

sub convert_solexa_quals_to_phred33{

  my $qual = shift;
  my @quals = split (//,$qual);
  my @new_quals;

  foreach my $index (0..$#quals){
    my $phred_score = convert_solexa_pre1_3_quality_string_into_phred_score ($quals[$index]);
    my $phred33_quality_string = convert_phred_score_into_phred33_quality_string ($phred_score);
    $new_quals[$index] = $phred33_quality_string;
  }

  my $phred33_quality = join ("",@new_quals);
  return $phred33_quality;
}

sub convert_phred_score_into_phred33_quality_string{
  my $qual = shift;
  $qual = chr($qual+33);
  return $qual;
}

sub convert_phred64_quality_string_into_phred_score{
  my $string = shift;
  my $qual = ord($string)-64;
  return $qual;
}

sub convert_solexa_pre1_3_quality_string_into_phred_score{
  ### We will just use 59 as the offset here as all Phred Scores between 10 and 40 look exactly the same, there is only a minute difference for values between 0 and 10
  my $string = shift;
  my $qual = ord($string)-59;
  return $qual;
}

#######################################################################################################################################
### Reset counters


sub reset_counters_and_fhs{
  %counting=(
	     sequences_count => 0,
	     no_single_alignment_found => 0,
	     unsuitable_sequence_count => 0,
	     genome_1_specific_count => 0,
	     genome_2_specific_count => 0,
	     unable_to_extract_genomic_sequence_count => 0,
	     aligns_to_both_genomes_equally_well_count => 0,
	     ambiguous_mapping_count => 0,
	     equivalent_sequences_dissimilar_count => 0,
	    );
  @fhs=(
	{ name => 'genome 1',
	  genome_index => $genome_index_basename_1,
	  seen => 0,
	},
	{ name => 'genome 2',
	  genome_index => $genome_index_basename_2,
	  seen => 0,
	},
       );
}

sub read_genome_1_into_memory{
  ## working directoy
  my $cwd = shift;
  ## reading in and storing the specified genome in %genome_1
  chdir ($genome_1) or die "Can't move to $genome_1: $!";
  print "Now reading in and storing sequence information of the genome specified in: $genome_1\n\n";

  my @chromosome_filenames =  <*.fa>;

  ### if there aren't any genomic files with the extension .fa we will look for files with the extension .fasta
  unless (@chromosome_filenames){
    @chromosome_filenames =  <*.fasta>;
  }

  unless (@chromosome_filenames){
    die "The specified genome folder $genome_1 does not contain any sequence files in FastA format (with .fa or .fasta file extensions)\n";
  }

  foreach my $chromosome_filename (@chromosome_filenames){


    ### skipping the TopHat whole genome fasta file
    next if ($chromosome_filename eq 'Mus_musculus.NCBIM37.fa');

    open (CHR_IN,$chromosome_filename) or die "Failed to read from sequence file $chromosome_filename $!\n";
    ### first line needs to be a fastA header
    my $first_line = <CHR_IN>;
    chomp $first_line;

    ### Extracting chromosome name from the FastA header
    my $chromosome_name = extract_chromosome_name($first_line);

    my $sequence;
    while (<CHR_IN>){
      chomp;
      if ($_ =~ /^>/){
	### storing the previous chromosome in the %genome_1 hash, only relevant for Multi-Fasta-Files (MFA)
	if (exists $genome_1{$chromosome_name}){
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name!\n";
	}
	else {
	  if (length($sequence) == 0){
	    warn "Chromosome $chromosome_name in the multi-fasta file $chromosome_filename did not contain any sequence information!\n";
	  }
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  $genome_1{$chromosome_name} = $sequence;
	}
	### resetting the sequence variable
	$sequence = '';
	### setting new chromosome name
	$chromosome_name = extract_chromosome_name($_);
      }
      else{
	$sequence .= uc$_;
      }
    }

    if (exists $genome_1{$chromosome_name}){
      print "chr $chromosome_name (",length $sequence ," bp)\t";
      die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name.\n";
    }
    else{
      if (length($sequence) == 0){
	warn "Chromosome $chromosome_name in the file $chromosome_filename did not contain any sequence information!\n";
      }
      print "chr $chromosome_name (",length $sequence ," bp)\n";
      $genome_1{$chromosome_name} = $sequence;
    }
  }
  print "\n";
  chdir $cwd or die "Failed to move to directory $cwd\n";
}

sub read_genome_2_into_memory{
  ## working directoy
  my $cwd = shift;
  ## reading in and storing the specified genome in %genome_2
  chdir ($genome_2) or die "Can't move to $genome_2: $!";
  print "Now reading in and storing sequence information of the genome specified in: $genome_2\n\n";

  my @chromosome_filenames =  <*.fa>;

  ### if there aren't any genomic files with the extension .fa we will look for files with the extension .fasta
  unless (@chromosome_filenames){
    @chromosome_filenames =  <*.fasta>;
  }

  unless (@chromosome_filenames){
    die "The specified genome folder $genome_2 does not contain any sequence files in FastA format (with .fa or .fasta file extensions)\n";
  }

  foreach my $chromosome_filename (@chromosome_filenames){

    ### skipping the TopHat whole genome fasta file
    next if ($chromosome_filename eq 'Mus_musculus.NCBIM37.fa');

    open (CHR_IN,$chromosome_filename) or die "Failed to read from sequence file $chromosome_filename $!\n";
    ### first line needs to be a fastA header
    my $first_line = <CHR_IN>;
    chomp $first_line;

    ### Extracting chromosome name from the FastA header
    my $chromosome_name = extract_chromosome_name($first_line);

    my $sequence;
    while (<CHR_IN>){
      chomp;
      if ($_ =~ /^>/){
	### storing the previous chromosome in the %genome_2 hash, only relevant for Multi-Fasta-Files (MFA)
	if (exists $genome_2{$chromosome_name}){
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name!\n";
	}
	else {
	  if (length($sequence) == 0){
	    warn "Chromosome $chromosome_name in the multi-fasta file $chromosome_filename did not contain any sequence information!\n";
	  }
	  print "chr $chromosome_name (",length $sequence ," bp)\n";
	  $genome_2{$chromosome_name} = $sequence;
	}
	### resetting the sequence variable
	$sequence = '';
	### setting new chromosome name
	$chromosome_name = extract_chromosome_name($_);
      }
      else{
	$sequence .= uc$_;
      }
    }
	
    if (exists $genome_2{$chromosome_name}){
      print "chr $chromosome_name (",length $sequence ," bp)\t";
      die "Exiting because chromosome name already exists. Please make sure all chromosomes have a unique name.\n";
    }
    else{
      if (length($sequence) == 0){
	warn "Chromosome $chromosome_name in the file $chromosome_filename did not contain any sequence information!\n";
      }
      print "chr $chromosome_name (",length $sequence ," bp)\n";
      $genome_2{$chromosome_name} = $sequence;
    }
  }
  print "\n";
  chdir $cwd or die "Failed to move to directory $cwd\n";
}


sub extract_chromosome_name {
    ## Bowtie seems to extract the first string after the inition > in the FASTA file, so we are doing this as well
    my $fasta_header = shift;
    if ($fasta_header =~ s/^>//){
	my ($chromosome_name) = split (/\s+/,$fasta_header);
	return $chromosome_name;
    }
    else{
	die "The specified chromosome ($fasta_header) file doesn't seem to be in FASTA format as required!\n";
    }
}

sub reverse_complement{
  my $sequence = shift;
  $sequence =~ tr/CATG/GTAC/;
  $sequence = reverse($sequence);
  return $sequence;
}


#######################################################################################################################################
### Process command line


sub process_command_line{
  my @bowtie_options;
  my $help;
  my $mates1;
  my $mates2;
  my $path_to_bowtie;
  my $fastq;
  my $fasta;
  my $skip;
  my $qupto;
  my $phred64;
  my $phred33;
  my $solexa;
  my $mismatches;
  my $seed_length;
  my $best;
  my $sequence_format;
  my $version;
  my $quiet;
  my $chunk;
  my $ceiling;
  my $maxins;
  my $minins;
  my $genome_1;
  my $genome_2;
  my $indexname_1;
  my $indexname_2;
  my $unmapped;
  my $dissimilar;

  my $command_line = GetOptions ('help|man' => \$help,
				 '1=s' => \$mates1,
				 '2=s' => \$mates2,
				 'path_to_bowtie=s' => \$path_to_bowtie,
				 'f|fasta' => \$fasta,
				 'q|fastq' => \$fastq,
				 's|skip=i' => \$skip,
				 'u|qupto=i' => \$qupto,
				 'phred33-quals' => \$phred33,
				 'phred64-quals|solexa1' => \$phred64,
				 'solexa-quals' => \$solexa,
				 'n|seedmms=i' => \$mismatches,
				 'l|seedlen=i' => \$seed_length,
				 'no_best' => \$best,
				 'version' => \$version,
				 'quiet' => \$quiet,
				 'chunkmbs=i' => \$chunk,
				 'I|minins=i' => \$minins,
				 'X|maxins=i' => \$maxins,
				 'e|maqerr=i' => \$ceiling,
				 'genome_1=s' => \$genome_1,
				 'genome_2=s' => \$genome_2,
				 'index_1=s' => \$indexname_1,
				 'index_2=s' => \$indexname_2,
				 'un|unmapped=s' => \$unmapped,
				 'dissimilar' => \$dissimilar,
				);


  ### EXIT ON ERROR if there were errors with any of the supplied options
  unless ($command_line){
    die "Please respecify command line options\n";
  }
  ### HELPFILE
  if ($help){
    print_helpfile();
    exit;
  }
  if ($version){
    print << "VERSION";


          ASAP - Allele Specific Alignment Program

   ASAP version: $ASAP_version Copyright 2011 Felix Krueger, Babraham Bioinformatics
              www.bioinformatics.bbsrc.ac.uk/projects/ASAP/


VERSION
    exit;
  }


  ##################################
  ### PROCESSING OPTIONS

  ### PATH TO BOWTIE
  ### if a special path to Bowtie was specified we will use that one, otherwise it is assumed that Bowtie is in the path
  if ($path_to_bowtie){
    unless ($path_to_bowtie =~ /\/$/){
      $path_to_bowtie =~ s/$/\//;
    }
    if (-d $path_to_bowtie){
      $path_to_bowtie = "${path_to_bowtie}bowtie";
    }
    else{
      die "The path to bowtie provided ($path_to_bowtie) is invalid (not a directory)!\n";
    }
  }
  else{
    $path_to_bowtie = 'bowtie';
  }
  print "Path to Bowtie specified as: $path_to_bowtie\n";

  ####################################
  ### PROCESSING ARGUMENTS


  ### GENOME FOLDERS

  unless ($genome_1){ # mandatory
    die "Genome 1 folder was not specified!\nUSAGE: ASAP [options] --genome_1 </path/> --genome_2 </path/> --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}\n";
  }

  unless ($genome_2){ # mandatory
    die "Genome 2 folder was not specified!\nUSAGE: ASAP [options] --genome_1 </path/> --genome_2 </path/> --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}\n";
  }

  ### checking that the genome folder, all subfolders and the required bowtie index files exist

  unless ($genome_1 =~/\/$/){
    $genome_1 =~ s/$/\//;
  }
  unless ($genome_2 =~/\/$/){
    $genome_2 =~ s/$/\//;
  }

  if (chdir $genome_1){
    print "The folder provided for reference genome 1 is $genome_1\n";
  }
  else{
    die "Failed to move to $genome_1: $!\n(--help for more details)\n";
  }

  if (chdir $genome_2){
    print "The folder provided for reference genome 2 is $genome_2\n";
  }
  else{
    die "Failed to move to $genome_2: $!\n(--help for more details)\n";
  }

  ### GENOME INDEX BASENAMES

  unless ($indexname_1 and $indexname_2){
    warn "You need to specify the bowtie index basenames of the two genomes to be aligned against!\nUSAGE: ASAP [options] --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}\n";
    exit;
  }

  ### checking if the required bowtie index files exist

  my @bowtie_index_1 = ($indexname_1.'.1.ebwt',$indexname_1.'.2.ebwt',$indexname_1.'.3.ebwt',$indexname_1.'.4.ebwt',$indexname_1.'.rev.1.ebwt',$indexname_1.'.rev.2.ebwt');
  foreach my $file(@bowtie_index_1){
    unless (-f $file){
      die "The bowtie index of the first genome seems to be faulty ($file). Please run bowtie-build before running ASAP\nUSAGE: ASAP [options] --genome_1 </path/> --genome_2 </path/> --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}\n";
    }
  }
  ### checking the integrity of $GA_dir
  my @bowtie_index_2 = ($indexname_2.'.1.ebwt',$indexname_2.'.2.ebwt',$indexname_2.'.3.ebwt',$indexname_2.'.4.ebwt',$indexname_2.'.rev.1.ebwt',$indexname_2.'.rev.2.ebwt');
  foreach my $file(@bowtie_index_2){
    unless (-f $file){
      die "The bowtie index of the second genome seems to be faulty ($file). Please run bowtie-build before running ASAP\nUSAGE: ASAP [options] --genome_1 </path/> --genome_2 </path/> --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}\n";
    }
  }

  ### INPUT OPTIONS

  ### SEQUENCE FILE FORMAT
  ### exits if both fastA and FastQ were specified
  if ($fasta and $fastq){
    die "Only one sequence filetype can be specified (fastA or fastQ)\n";
  }

  ### unless fastA is specified explicitely, fastQ sequence format is expected by default
  if ($fasta){
    print "FastA format specified\n";
    $sequence_format = 'FASTA';
    push @bowtie_options, '-f';
  }
  elsif ($fastq){
    print "FastQ format specified\n";
    $sequence_format = 'FASTQ';
    push @bowtie_options, '-q';
  }
  else{
    $fastq=1;
    print "FastQ format assumed (default)\n";
    $sequence_format = 'FASTQ';
    push @bowtie_options, '-q';
  }

  ### SKIP
  if ($skip){
    push @bowtie_options,"-s $skip";
  }

  ### UPTO
  if ($qupto){
    push @bowtie_options,"--qupto $qupto";
  }

  ### QUALITY VALUES
  if (($phred33 and $phred64) or ($phred33 and $solexa) or ($phred64 and $solexa)){
    die "You can only specify one type of quality value at a time! (--phred33-quals or --phred64-quals or --solexa-quals)";
  }
  if ($phred33){
    # Phred quality values work only when -q is specified
    unless ($fastq){
      die "Phred quality values works only when -q (FASTQ) is specified\n";
    }
    push @bowtie_options,"--phred33-quals";
  }
  if ($phred64){
    # Phred quality values work only when -q is specified
    unless ($fastq){
      die "Phred quality values work only when -q (FASTQ) is specified\n";
    }
    push @bowtie_options,"--phred64-quals";
  }
  else{
    $phred64 = 0;
  }

  if ($solexa){
    # Solexa to Phred value conversion works only when -q is specified
    unless ($fastq){
      die "Conversion from Solexa to Phred quality values works only when -q (FASTQ) is specified\n";
    }
    push @bowtie_options,"--solexa-quals";
  }
  else{
    $solexa = 0;
  }

  ### ALIGNMENT OPTIONS

  ### MISMATCHES
  if (defined $mismatches){
    push @bowtie_options,"-n $mismatches";
  }
  ### SEED LENGTH
  if (defined $seed_length){
    push @bowtie_options,"-l $seed_length";
  }
  ### MISMATCH CEILING
  if (defined $ceiling){
    push @bowtie_options,"-e $ceiling";
  }

  ### REPORTING OPTIONS
  # Because of the way ASAP works we will always use the reporting option -k 2 (report up to 2 valid alignments)
  push @bowtie_options,'-k 2';

  ### --BEST
  # --best will be the default option, specifying --no-best can turn it off (e.g. to speed up alignment process)
  unless ($best){
    push @bowtie_options,'--best';
  }

  ### PAIRED-END MAPPING
  if ($mates1){
    my @mates1 = (split (/,/,$mates1));
    die "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!\n" unless ($mates2);
    my @mates2 = (split(/,/,$mates2));
    unless (scalar @mates1 == scalar @mates2){
      die "Paired-end mapping requires the same amounnt of mate1 and mate2 files, please respecify! (format: -1 <mates1> -2 <mates2>)\n";
    }
    while (1){
      my $mate1 = shift @mates1;
      my $mate2 = shift @mates2;
      last unless ($mate1 and $mate2);
      push @filenames,"$mate1,$mate2";
    }
  }
  elsif ($mates2){
    die "Paired-end mapping requires the format: -1 <mates1> -2 <mates2>, please respecify!\n";
  }

  ### SINGLE-END MAPPING
  # Single-end mapping will be performed if no mate pairs for paired-end mapping have been specified
  my $singles;
  unless ($mates1 and $mates2){
    $singles = shift @ARGV;
    unless ($singles){
      die "\nNo filename supplied! Please specify one or more files for single-end ASAP mapping!\n";
    }
    @filenames = (split(/,/,$singles));
  }

  ### MININUM INSERT SIZE (PAIRED-END ONLY)
  if (defined $minins){
    die "-I/--minins can only be used for paired-end mapping!\n\n" if ($singles);
    push @bowtie_options,"--minins $minins";
  }

  ### MAXIMUM INSERT SIZE (PAIRED-END ONLY)
  if (defined $maxins){
    die "-X/--maxins can only be used for paired-end mapping!\n\n" if ($singles);
    push @bowtie_options,"--maxins $maxins";
  }

  ### QUIET prints nothing  besides alignments (suppresses warnings)
  if ($quiet){
    push @bowtie_options,'--quiet';
  }

  ### CHUNKMBS needed to be increased to avoid memory exhaustion warnings, particularly for --best (and paired-end) alignments
  if (defined $chunk){
    push @bowtie_options,"--chunkmbs $chunk";
  }

  ### SUMMARY OF ALL BOWTIE OPTIONS
  my $bowtie_options = join (' ',@bowtie_options);


  ### UNMAPPED SEQUENCE OUTPUT
  if ($unmapped){
    unless ($unmapped =~ /\.\w+$/){
      die "Please provide a filename for unmapped sequences in the format: filename.extension (e.g. unaligned.txt)\n";
    }
    die "The file to output unmapped sequences already exists. Please specify a new name\n" if (-e "$parent_dir/$unmapped");
    if ($mates1 and $mates2){
      my $un1 = my $un2 = $unmapped;
      $un1 =~ s/(\.\w+)$/_1$1/;
      $un2 =~ s/(\.\w+)$/_2$1/;
      if (-e "$parent_dir/$un1" or -e "$parent_dir/$un2"){
	die "The specified output files for unmapped sequences already exist. Please specify a new name\n";
      }
    }
  }
  else{
    $unmapped = 0;
  }

  ### DISSIMILAR GENOMES
  if ($dissimilar){
    print "Dissimilar genomes selected. Note that alignments in common output file will be different to normal mode.\n";
  }
  else{
    $dissimilar = 0;
  }

  return ($indexname_1,$indexname_2,$genome_1,$genome_2,$path_to_bowtie,$sequence_format,$bowtie_options,$unmapped,$dissimilar,$phred64,$solexa);
}


#######################################################################################################################################
### Helpfile

sub print_helpfile{
  print << "HOW_TO";


     This program is free software: you can redistribute it and/or modify
     it under the terms of the GNU General Public License as published by
     the Free Software Foundation, either version 3 of the License, or
     (at your option) any later version.

     This program is distributed in the hope that it will be useful,
     but WITHOUT ANY WARRANTY; without even the implied warranty of
     MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
     GNU General Public License for more details.
     You should have received a copy of the GNU General Public License
     along with this program.  If not, see <http://www.gnu.org/licenses/>.



DESCRIPTION


The following is a brief description of command line options and arguments to control the allele specific
alignment pipeline, ASAP. Bismark takes in FastA or FastQ files and aligns the
reads to a specified bisulfite genome. We are going to take sequence reads and transform the sequence
into a bisulfite converted forward strand (C->T conversion) or into a bisulfite treated reverse strand
(G->A conversion of the forward strand). We then align each of these reads to bisulfite treated forward
strand index of the mouse genome (C->T converted) and a bisulfite treated reverse strand index of the
genome (G->A conversion on the forward strand, by doing this alignments will produce the same positions).
These 4 instances of bowtie will be run in parallel. We are then going to read in the sequence file again
line by line to pull out the original sequence from the mouse genome and determine if there were any
protected C's present or not. We are then going to print out the methylation calls into a final result file.

The final Bismark output of this script will be a single tab delimited file with all sequences that have
a unique best alignment to any of the 4 possible strands of a bisulfite PCR product. The format is described 
in more detail below.

USAGE: ASAP [options] --genome_1 </path/> --genome_2 </path/> --index_1 <genome_index_1> --index_2 <genome_index_2> {-1 <mates1> -2 <mates2> | <singles>}


ARGUMENTS:

--genome_1 <>            The full path to the folder containing reference genome 1. ASAP expects one or
                         more FastA files in this folder (file extension: .fa).

--genome_2 <>            The full path to the folder containing reference genome 2. ASAP expects one or
                         more FastA files in this folder (file extension: .fa).

--index_1 <>             The full path to the bowtie index base name of genome 1 (e.g.
                         /data/genomes/mouse/mus_musculus/C57BL6).

--index_2 <>             The full path to the bowtie index base name of genome 2 (e.g.
                         /data/genomes/mouse/mus_musculus/castaneus).

-1 <mates1>              Comma-separated list of files containing the #1 mates (filename usually includes
                         "_1"), e.g. flyA_1.fq,flyB_1.fq). Sequences specified with this option must
                         correspond file-for-file and read-for-read with those specified in <mates2>.
                         Reads may be a mix of different lengths. Bismark will produce one mapping result
                         and one report file per paired-end input file pair.

-2 <mates2>              Comma-separated list of files containing the #2 mates (filename usually includes
                         "_2"), e.g. flyA_1.fq,flyB_1.fq). Sequences specified with this option must
                         correspond file-for-file and read-for-read with those specified in <mates1>.
                         Reads may be a mix of different lengths.

<singles>                A comma-separated list of files containing the reads to be aligned (e.g. lane1.fq,
                         lane2.fq,lane3.fq). Reads may be a mix of different lengths. ASAP will produce
                         one mapping result and one report file per input file.


OPTIONS:


Input:

-q/--fastq               The query input files (specified as <mate1>,<mate2> or <singles> are FASTQ
                         files (usually having extension .fg or .fastq). This is the default. See also
                         --solexa-quals and --integer-quals.

-f/--fasta               The query input files (specified as <mate1>,<mate2> or <singles> are FASTA
                         files (usually havin extension .fa, .mfa, .fna or similar). All quality values
                         are assumed to be 40 on the Phred scale.

-s/--skip <int>          Skip (i.e. do not align) the first <int> reads or read pairs from the input.

-u/--qupto <int>         Only aligns the first <int> reads or read pairs from the input. Default: no limit.

--phred33-quals          FASTQ qualities are ASCII chars equal to the Phred quality plus 33. Default: on.

--phred64-quals          FASTQ qualities are ASCII chars equal to the Phred quality plus 64. Default: off.

--solexa-quals           Convert FASTQ qualities from solexa-scaled (which can be negative) to phred-scaled
                         (which can't). The formula for conversion is: 
                         phred-qual = 10 * log(1 + 10 ** (solexa-qual/10.0)) / log(10). Used with -q. This
                         is usually the right option for use with (unconverted) reads emitted by the GA
                         Pipeline versions prior to 1.3. Default: off.

--solexa1.3-quals        Same as --phred64-quals. This is usually the right option for use with (unconverted)
                         reads emitted by GA Pipeline version 1.3 or later. Default: off.

--path_to_bowtie         The full path </../../> to the Bowtie installation on your system. If not specified
                         it will be assumed that Bowtie is in the path.

--dissimilar             Specifying this option will inform ASAP that the two genomes are not essentially the
                         same except for SNPs (which is the default), but that they are dissimilar (e.g.
                         genome 1 could be the Black6 mouse genome, and genome 2 could be just one chromosome
                         from a different mouse strain which can potentially include SNPs and/or chromosomal
                         rearrangements). In such a case, ASAP will not attempt to extract the genomic sequence
                         at the corresponding position in the second genome, but will write out the first best
                         alignment to the second genome instead (if appplicable; if there was no best alignment
                         genome 2 fields will be left blank). This option will not write any sequences to mixed
                         output file as the concept of homologous sequences is not applicable.


Alignment:

-n/--seedmms <int>       The maximum number of mismatches permitted in the "seed", which is the first 20
                         base pairs of the read by default (see -l/--seedlen). This may be 0, 1, 2 or 3.

-l/--seedlen             The "seed length"; i.e., the number of bases of the high quality end of the read to
                         which the -n ceiling applies. The default is 28.

-e/--maqerr <int>        Maximum permitted total of quality values at all mismatched read positions throughout
                         the entire alignment, not just in the "seed". The default is 70. Like Maq, bowtie rounds
                         quality values to the nearest 10 and saturates at 30.

--chunkmbs <int>         The number of megabytes of memory a given thread is given to store path descriptors in 
                         --best mode. Best-first search must keep track of many paths at once to ensure it is 
                         always extending the path with the lowest cumulative cost. Bowtie tries to minimize the 
                         memory impact of the descriptors, but they can still grow very large in some cases. If 
                         you receive an error message saying that chunk memory has been exhausted in --best mode,
                         try adjusting this parameter up to dedicate more memory to the descriptors. Default: 64.

-I/--minins <int>        The minimum insert size for valid paired-end alignments. E.g. if -I 60 is specified and
                         a paired-end alignment consists of two 20-bp alignments in the appropriate orientation
                         with a 20-bp gap between them, that alignment is considered valid (as long as -X is also
                         satisfied). A 19-bp gap would not be valid in that case. Default: 0.

-X/--maxins <int>        The maximum insert size for valid paired-end alignments. E.g. if -X 100 is specified and
                         a paired-end alignment consists of two 20-bp alignments in the proper orientation with a
                         60-bp gap between them, that alignment is considered valid (as long as -I is also satisfied).
                         A 61-bp gap would not be valid in that case. Default: 250.

Reporting:

-k <2>                   Due to the way ASAP works Bowtie will report up to 2 valid alignments. This option
                         will be used by default.

--best                   Make Bowtie guarantee that reported singleton alignments are "best" in terms of stratum
                         (i.e. number of mismatches, or mismatches in the seed in the case if -n mode) and in
                         terms of the quality; e.g. a 1-mismatch alignment where the mismatch position has Phred
                         quality 40 is preferred over a 2-mismatch alignment where the mismatched positions both
                         have Phred quality 10. When --best is not specified, Bowtie may report alignments that
                         are sub-optimal in terms of stratum and/or quality (though an effort is made to report
                         the best alignment). --best mode also removes all strand bias. Note that --best does not
                         affect which alignments are considered "valid" by Bowtie, only which valid alignments
                         are reported by Bowtie. Bowtie is about 1-2.5 times slower when --best is specified.
                         Default: on.

--no_best                Disables the --best option which is on by default. This can speed up the alignment process,
                         e.g. for testing purposes, but for credible results it is not recommended to disable --best.

--quiet                  Print nothing besides alignments.


Other:

-h/--help                Displays this help file.

-v/--version             Displays version information.



OUTPUT:

Single-end output format (tab-separated):

  (1) seq-ID
  (2) read sequence
  (3) specific for genome                      [1/2/N]
  (4) read alignment strand                    [+/-]
  (5) alignment
  (6) alignment start position
  (7) alignment end position
  (8) genome 1 sequence
  (9) genome 1 mismatch info                   [blank if perfect match]
 (10) genome 2 sequence
 (11) genome 2 mismatch info                   [blank if perfect match]
 (12) read quality score (Phred33 scale, Sanger encoding)

Paired-end output format (tab-separated):

  (1) seq-ID
  (2) read 1 sequence
  (3) read 2 sequence
  (4) specific for genome                      [1/2/N]
  (5) read 1 alignment strand                  [+/-]
  (6) alignment chromosome
  (7) alignment start position
  (8) alignment end position
  (9) genome 1 sequence 1
 (10) genome 1 read 1 mismatch information     [blank if perfect match]
 (11) genome 1 sequence 2
 (12) genome 1 read 2 mismatch information     [blank if perfect match]
 (13) genome 2 sequence 1
 (14) genome 2 read 1 mismatch information     [blank if perfect match]
 (15) genome 2 sequence 2
 (16) genome 2 read 2 mismatch information     [blank if perfect match]
 (17) read 1 quality score (Phred33 scale, Sanger encoding)
 (18) read 2 quality score (Phred33 scale, Sanger encoding)


This script was last edited on 2 Jun 2011.

HOW_TO
}
