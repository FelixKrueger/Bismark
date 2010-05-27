#!/usr/bin/perl
use warnings;
use strict;
$|++;
use Getopt::Long;


my @filenames;
my %counting;
my %fhs;
my ($ignore,$genomic_fasta,$single,$paired) = process_commandline();

process_Bismark_results_file($ignore,$single,$paired);

sub process_commandline{
  my $help;
  my $single_end;
  my $paired_end;
  my $ignore;
  my $genomic_fasta;

  my $command_line = GetOptions ('help|man' => \$help,
				 'p|paired-end' => \$paired_end,
				 's|single-end' => \$single_end,
				 'fasta' => \$genomic_fasta,
				 'ignore=i' => \$ignore,
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

  ### no files provided
  unless (@ARGV){
    die "You need to provide one or more files in Bismark format to create an individual C methylation output.\n";
  }
  @filenames = @ARGV;


  ### IGNORING <INT> bases at the start of the read when processing the methylation call string
  if ($ignore){
    warn "First $ignore bases will be disregarded when processing the methylation call string\n";
  }
  else {
    $ignore = 0;
  }
  sleep (5);

  ### SINGLE END ALIGNMENTS
  if ($single_end){
    print "Bismark Single-End format specified\n";
    $paired_end = 0;
  }

  ### PAIRED-END ALIGNMENTS
  elsif ($paired_end){
    print "Bismark Paired-End format specified\n";
    $single_end = 0;
  }

  else{
    die "Please specify whether the supplied file(s) are in Bismark single-end or paired-end format\n\n";
  }

  return ($ignore,$genomic_fasta,$single_end,$paired_end);
}



sub process_Bismark_results_file{
  my ($ignore,$single,$paired) = @_;
  print "ignore: $ignore\nsingle: $single\npaired: $paired\n";
  sleep (5);
  foreach my $filename (@filenames){
    %fhs = ();
    ###creating CpG and non-CpG output filehandles
    $fhs{0}->{name} = 'OT';
    $fhs{1}->{name} = 'CTOT';
    $fhs{2}->{name} = 'CTOB';
    $fhs{3}->{name} = 'OB';
    %counting =(
		total_meC_count => 0,
		total_meCpG_count => 0,
		total_unmethylated_C_count => 0,
		total_unmethylated_CpG_count => 0,
		sequences_count => 0,
	       );
    print "\nNow reading in Bismark result file $filename\n";
    open (IN,$filename) or die "Can't open file $!\n";

    my $cpg_ot = my $cpg_ctot = my $cpg_ctob = my $cpg_ob = $filename;

    ### OPENING OUT-FILEHANDLES

    ### For cytosines in CpG context
    if ($cpg_ot =~ s/^/CpG_OT_/){
      open ($fhs{0}->{CpG},'>',$cpg_ot) or die "Failed to write to $cpg_ot $!\n";
      print "Writing result file containing methylation information for C in CpG context from the original forward strand to $cpg_ot\n";
    }
    if ($cpg_ctot =~ s/^/CpG_CTOT_/){
      open ($fhs{1}->{CpG},'>',$cpg_ctot) or die "Failed to write to $cpg_ctot $!\n";
      print "Writing result file containing methylation information for C in CpG context from the complementary to original forward strand to $cpg_ctot\n";
    }
    if ($cpg_ctob =~ s/^/CpG_CTOB_/){
      open ($fhs{2}->{CpG},'>',$cpg_ctob) or die "Failed to write to $cpg_ctob $!\n";
      print "Writing result file containing methylation information for C in CpG context from the complementary to original reverse strand to $cpg_ctob\n";
    }
    if ($cpg_ob =~ s/^/CpG_OB_/){
      open ($fhs{3}->{CpG},'>',$cpg_ob) or die "Failed to write to $cpg_ob $!\n";
      print "Writing result file containing methylation information for C in CpG context from the original reverse strand to $cpg_ob\n";
    }

    ### For cytosines in CC, CT or CA context
    my $other_c_ot = my $other_c_ctot = my $other_c_ctob = my $other_c_ob = $filename;
    if ($other_c_ot =~ s/^/Other_C_OT_/){
      open ($fhs{0}->{other_c},'>',$other_c_ot) or die "Failed to write to $other_c_ot $!\n";
      print "Writing result file containing methylation information for C in any other context from the original forward strand to $other_c_ot\n";
    }
    if ($other_c_ctot =~ s/^/Other_C_CTOT_/){
      open ($fhs{1}->{other_c},'>',$other_c_ctot) or die "Failed to write to $other_c_ctot $!\n";
      print "Writing result file containing methylation information for C in any other context from the complementary to original forward strand to $other_c_ctot\n";
    }
    if ($other_c_ctob =~ s/^/Other_C_CTOB_/){
      open ($fhs{2}->{other_c},'>',$other_c_ctob) or die "Failed to write to $other_c_ctob $!\n";
      print "Writing result file containing methylation information for C in any other context from the complementary to original reverse strand to $other_c_ctob\n";
    }
    if ($other_c_ob =~ s/^/Other_C_OB_/){
      open ($fhs{3}->{other_c},'>',$other_c_ob) or die "Failed to write to $other_c_ob $!\n";
      print "Writing result file containing methylation information for C in any other context from the original reverse strand to $other_c_ob\n";
    }

    ### For repeat analyses or similar one can obtain a FastA output file with the genomic equivalent sequences for a bisulfite read position
    if ($genomic_fasta){
      my $fasta = $filename;
      $fasta =~ s/^/genomic_equivalents_fastA_/;
      open (FASTA,'>',$fasta) or die "Can't write to file $fasta: $!\n";
    }
    my $methylation_call_strings_processed = 0;
    my $line_count = 0;

    ### proceeding differently now for single-end or paired-end Bismark files

    ### PROCESSING SINGLE-END RESULT FILES
    if ($single){
      while (<IN>){
	++$line_count;
	print "processed lines: $line_count\n" if ($line_count%500000==0);
	
	### $seq here is the chromosomal sequence (to use for the repeat analysis for example)
	my ($id,$strand,$chrom,$start,$seq,$meth_call,$index,$conversion_info) = (split("\t"))[0,1,2,3,5,6,7,8];
	### we need to remove 1 bp of the genomic sequence as we were extracting 41 bp long fragments to make a methylation call at the first or last position
	if ($meth_call){
	
	  ### We will need to discriminate between 1 extra base at the 5' end or at the 3' end
	  ### removing most 3' base
	  if ($conversion_info =~ /^CT/){
	    $seq = substr($seq,0,length($seq)-1);
	  }	
	  ### removing most 5' base
	  elsif ($conversion_info =~ /^GA/){
	    $seq = substr($seq,1,length($seq)-1);
	  }
	  else{
	    die "We need the read conversion info to proceed with extracting the correct part of the genomic sequence\n";
	  }

	  ### Clipping off the first <int> number of bases from the methylation call string as specified with --ignore <int>
	  if ($ignore){
	    $meth_call = substr($meth_call,$ignore,length($meth_call)-$ignore);	
	  }

	  ### printing out the methylation state of every C in the read
	  print_individual_C_methylation_states_single_end($meth_call,$chrom,$start,$id,$seq,$strand,$index);

	  ### if $genomic_fasta has been specified we print out a FastA file with genomic equivalent sequences
	  if ($genomic_fasta){
	    print FASTA ">$line_count\n";
	    print FASTA "$seq\n";
	  }
	  ++$methylation_call_strings_processed; # 1 per single-end result
	}
      }
    }

    ### PROCESSING PAIRED-END RESULT FILES
    elsif ($paired){
      print "nothing really\n";

      while (<IN>){
	++$line_count;
	print "processed $line_count lines\n" if ($line_count%500000==0);
	my ($id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2,$index) = (split("\t"))[0,2,3,4,6,7,9,10,11];
	### we need to remove the last base of the genomic sequence as we were extracting 41 bp long fragments to make a methylation call at the 40th position
	##these substrings need to be thought through again, it depends on whether there is a leading or a trailing base (CT or GA conversion, respectively)
	$seq_1 = substr($seq_1,0,40);
	$seq_2 = substr($seq_2,0,40);
	$start_read_1 += 1; ### doing this because bowtie reports the index and not the base pair position of the the start sequence
	if ($index == 0 or $index == 1){
	  my $end_read_1 = $start_read_1+length($seq_1)-1;
	  my $start_read_2 = $end_read_2-length($seq_2)+1;
	  # print join ("\t",$id,$chrom,$start_read_1,$end_read_1,$seq_1,$meth_call_1),"\n";
	  # print join ("\t",$id,$chrom,$start_read_2,$end_read_2,$seq_2,$meth_call_2),"\n";
	  ### print_fastA_file_with_genomic_equivalent_sequences($id,$chrom,$start_read_1,$seq_1,$end_read_2,$seq_1);
	  # print join ("\t",$id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2),"\n";
	  ## we first pass the first read of a paired-end alignment
	  print_individual_C_methylation_states_paired_end_files($meth_call_1,$chrom,$start_read_1,$id,'+',$index);
	  # we next pass the second read, which is always in - orientation on the reverse strand
	  print_individual_C_methylation_states_paired_end_files($meth_call_2,$chrom,$end_read_2,$id,'-',$index);
	  $counting{sequences_count}++;
	}
	elsif ($index == 2 or $index == 3){
	  my $end_read_1 = $start_read_1+length($seq_1)-1;
	  my $start_read_2 = $end_read_2-length($seq_2)+1;
	  # print join ("\t",$id,$chrom,$start_read_1,$end_read_1,$seq_1,$meth_call_1),"\n";
	  # print join ("\t",$id,$chrom,$start_read_2,$end_read_2,$seq_2,$meth_call_2),"\n";
	  ### print_fastA_file_with_genomic_equivalent_sequences($id,$chrom,$start_read_1,$seq_1,$end_read_2,$seq_1);
	  # print join ("\t",$id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2),"\n";
	  ## we first pass the first read of a paired-end alignment
	  
	  ### I AM JUST PASSING ON THE METHYLATION CALL FROM THE OTHER READ. ALTHOUGH THIS SHOULD FIX THE PROBLEM I NEED A MORE LONG TERM SOLUTION!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	  print_individual_C_methylation_states_paired_end_files($meth_call_2,$chrom,$start_read_1,$id,'+',$index);
	  # we next pass the second read, which is always in - orientation on the reverse strand
	  print_individual_C_methylation_states_paired_end_files($meth_call_1,$chrom,$end_read_2,$id,'-',$index);
	  ++$methylation_call_strings_processed; # paired-end = 2 methylation calls
	  $counting{sequences_count}++;
	}
	else{
	  die "There can only be 4 different index numbers\n";
	}
	
	++$methylation_call_strings_processed; # paired-end = 2 methylation calls
      }
    }
    else{
      die "Single-end or paired-end reads not specified properly: $!\n";
    }

    print "Processed $line_count lines from $filename in total\n";
    print "Total number of methylation call strings processed: $methylation_call_strings_processed\n";
    ### detailed information about Cs analysed
    print "Final Cytosine Methylation Report\n",'='x33,"\n";
    my $total_number_of_C = $counting{total_meC_count}+$counting{total_meCpG_count}+$counting{total_unmethylated_C_count}+$counting{total_unmethylated_CpG_count};
    print "Total number of C's analysed:\t$total_number_of_C\n";
    print "Total methylated C's in non-CpG context:\t$counting{total_meC_count}\n";
    print "Total methylated C's in CpG context:\t $counting{total_meCpG_count}\n";
    print "Total C to T conversions in non-CpG context:\t$counting{total_unmethylated_C_count}\n";
      print "Total C to T conversions in CpG context:\t$counting{total_unmethylated_CpG_count}\n\n";
    my $percent_meC;
    if (($counting{total_meC_count}+$counting{total_unmethylated_C_count}) > 0){
      $percent_meC = sprintf("%.1f",100*$counting{total_meC_count}/($counting{total_meC_count}+$counting{total_unmethylated_C_count}));
    }
    my $percent_meCpG;
    if (($counting{total_meCpG_count}+$counting{total_unmethylated_CpG_count}) > 0){
      $percent_meCpG = sprintf("%.1f",100*$counting{total_meCpG_count}/($counting{total_meCpG_count}+$counting{total_unmethylated_CpG_count}));
    }
    ### calculating methylated C percentage (non CpG context) if applicable
    if ($percent_meC){
      print "C methylated but not in CpG context:\t${percent_meC}%\n";
    }
    else{
      print "Can't determine percentage of methylated Cs (not in CpG context) if value was 0\n";
    }
    ### calculating methylated CpG percentage if applicable
    if ($percent_meCpG){
      print "C methylated in CpG context:\t${percent_meCpG}%\n\n\n";
    }
    else{
      print "Can't determine percentage of methylated Cs (in CpG context) if value was 0\n\n\n";
    }
  }
}


sub process_paired_end_Bismark_results_file{
  foreach my $filename (@filenames){
    %fhs =();
    %counting =(
		total_meC_count => 0,
		total_meCpG_count => 0,
		total_unmethylated_C_count => 0,
		total_unmethylated_CpG_count => 0,
		sequences_count => 0,
	       );
    print "Now reading in paired-end BiSeq result file $filename\n";
    open (IN,$filename) or die "Can't open file $!\n";
    my $fasta = $filename;
    # $fasta =~ s/^/genomic_equivalents_/;
    # $fasta =~ s/txt$/fa/;
    # open (FASTA,'>',$fasta) or die "Can't write to file $!\n";
    my $count =0;
    my $cpg_ot = my $cpg_ctot = my $cpg_ctob = my $cpg_ob = $filename;
    ###creating a hash with CpG and non-CpG outout filehandles
    $fhs{0}->{name} = 'OT';
    $fhs{1}->{name} = 'CTOT';
    $fhs{2}->{name} = 'CTOB';
    $fhs{3}->{name} = 'OB';
    if ($cpg_ot =~ s/^/CpG_OT_/){
      open ($fhs{0}->{CpG},'>',$cpg_ot) or die "Failed to write to $cpg_ot $!\n";
      print "Writing result file containing methylation information for C in CpG context from the original forward strand to $cpg_ot\n";
    }
    if ($cpg_ctot =~ s/^/CpG_CTOT_/){
      open ($fhs{1}->{CpG},'>',$cpg_ctot) or die "Failed to write to $cpg_ctot $!\n";
      print "Writing result file containing methylation information for C in CpG context from the complementary to original forward strand to $cpg_ctot\n";
    }
    if ($cpg_ctob =~ s/^/CpG_CTOB_/){
      open ($fhs{2}->{CpG},'>',$cpg_ctob) or die "Failed to write to $cpg_ctob $!\n";
      print "Writing result file containing methylation information for C in CpG context from the complementary to original reverse strand to $cpg_ctob\n";
    }
    if ($cpg_ob =~ s/^/CpG_OB_/){
      open ($fhs{3}->{CpG},'>',$cpg_ob) or die "Failed to write to $cpg_ob $!\n";
      print "Writing result file containing methylation information for C in CpG context from the original reverse strand to $cpg_ob\n";
    }
    my $other_c_ot = my $other_c_ctot = my $other_c_ctob = my $other_c_ob = $filename;
    if ($other_c_ot =~ s/^/Other_C_OT_/){
      open ($fhs{0}->{other_c},'>',$other_c_ot) or die "Failed to write to $other_c_ot $!\n";
      print "Writing result file containing methylation information for C in any other context from the original forward strand to $other_c_ot\n";
    }
    if ($other_c_ctot =~ s/^/Other_C_CTOT_/){
      open ($fhs{1}->{other_c},'>',$other_c_ctot) or die "Failed to write to $other_c_ctot $!\n";
     print "Writing result file containing methylation information for C in any other context from the complementary to original forward strand to $other_c_ctot\n";
    }
    if ($other_c_ctob =~ s/^/Other_C_CTOB_/){
      open ($fhs{2}->{other_c},'>',$other_c_ctob) or die "Failed to write to $other_c_ctob $!\n";
      print "Writing result file containing methylation information for C in any other context from the complementary to original reverse strand to $other_c_ctob\n";
    }
    if ($other_c_ob =~ s/^/Other_C_OB_/){
      open ($fhs{3}->{other_c},'>',$other_c_ob) or die "Failed to write to $other_c_ob $!\n";
      print "Writing result file containing methylation information for C in any other context from the original reverse strand to $other_c_ob\n";
    }
    while (<IN>){
      #  last if ($count == 10000);
      print "processed $count lines\n" if ($count%500000==0);
      my ($id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2,$index) = (split("\t"))[0,2,3,4,6,7,9,10,11];
      ### we need to remove the last base of the genomic sequence as we were extracting 41 bp long fragments to make a methylation call at the 40th position
      ##these substrings need to be thought through again, it depends on whether there is a leading or a trailing base (CT or GA conversion, respectively)
      $seq_1 = substr($seq_1,0,40);
      $seq_2 = substr($seq_2,0,40);
      $start_read_1 += 1; ### doing this because bowtie reports the index and not the base pair position of the the start sequence
      if ($index == 0 or $index == 1){
	my $end_read_1 = $start_read_1+length($seq_1)-1;
	my $start_read_2 = $end_read_2-length($seq_2)+1;
	# print join ("\t",$id,$chrom,$start_read_1,$end_read_1,$seq_1,$meth_call_1),"\n";
	# print join ("\t",$id,$chrom,$start_read_2,$end_read_2,$seq_2,$meth_call_2),"\n";
	### print_fastA_file_with_genomic_equivalent_sequences($id,$chrom,$start_read_1,$seq_1,$end_read_2,$seq_1);
	# print join ("\t",$id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2),"\n";
	## we first pass the first read of a paired-end alignment
	print_individual_C_methylation_states_paired_end_files($meth_call_1,$chrom,$start_read_1,$id,'+',$index);
	# we next pass the second read, which is always in - orientation on the reverse strand
	print_individual_C_methylation_states_paired_end_files($meth_call_2,$chrom,$end_read_2,$id,'-',$index);
	$count += 2; # paired-end = 2 sequences
	$counting{sequences_count}++;
      }
      elsif ($index == 2 or $index == 3){
	my $end_read_1 = $start_read_1+length($seq_1)-1;
	my $start_read_2 = $end_read_2-length($seq_2)+1;
	# print join ("\t",$id,$chrom,$start_read_1,$end_read_1,$seq_1,$meth_call_1),"\n";
	# print join ("\t",$id,$chrom,$start_read_2,$end_read_2,$seq_2,$meth_call_2),"\n";
	### print_fastA_file_with_genomic_equivalent_sequences($id,$chrom,$start_read_1,$seq_1,$end_read_2,$seq_1);
	# print join ("\t",$id,$chrom,$start_read_1,$end_read_2,$seq_1,$meth_call_1,$seq_2,$meth_call_2),"\n";
	## we first pass the first read of a paired-end alignment

	### I AM JUST PASSING ON THE METHYLATION CALL FROM THE OTHER READ. ALTHOUGH THIS SHOULD FIX THE PROBLEM I NEED A MORE LONG TERM SOLUTION!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
	print_individual_C_methylation_states_paired_end_files($meth_call_2,$chrom,$start_read_1,$id,'+',$index);
	# we next pass the second read, which is always in - orientation on the reverse strand
	print_individual_C_methylation_states_paired_end_files($meth_call_1,$chrom,$end_read_2,$id,'-',$index);
	$count += 2; # paired-end = 2 sequences
	$counting{sequences_count}++;
	if ($genomic_fasta){	
	  print FASTA ">$id,$chrom,$start_read_1\n";
	  print FASTA "$seq_1\n";
	  print FASTA ">$id,$chrom,$end_read_2\n";
	  print FASTA "$seq_2\n";
	}
      }
      else{
	warn "There can only be 4 different index numbers\n";
      }
    }
    print "Processed $count lines from $filename in total\n\n";
    ### detailed information about Cs analysed
    print "Final Cytosine Methylation Report\n",'='x33,"\n";
    my $total_number_of_C = $counting{total_meC_count}+$counting{total_meCpG_count}+$counting{total_unmethylated_C_count}+$counting{total_unmethylated_CpG_count};
    print "Total number of C's analysed:\t$total_number_of_C\n";
    print "Total methylated C's in non-CpG context:\t$counting{total_meC_count}\n";
    print "Total methylated C's in CpG context:\t $counting{total_meCpG_count}\n";
    print "Total C to T conversions in non-CpG context:\t$counting{total_unmethylated_C_count}\n";
    print "Total C to T conversions in CpG context:\t$counting{total_unmethylated_CpG_count}\n\n";
    my $percent_meC;
    if (($counting{total_meC_count}+$counting{total_unmethylated_C_count}) > 0){
      $percent_meC = sprintf("%.1f",100*$counting{total_meC_count}/($counting{total_meC_count}+$counting{total_unmethylated_C_count}));
    }
    my $percent_meCpG;
    if (($counting{total_meCpG_count}+$counting{total_unmethylated_CpG_count}) > 0){
      $percent_meCpG = sprintf("%.1f",100*$counting{total_meCpG_count}/($counting{total_meCpG_count}+$counting{total_unmethylated_CpG_count}));
    }
    ### calculating methylated C percentage (non CpG context) if applicable
    if ($percent_meC){
      print "C methylated but not in CpG context:\t${percent_meC}%\n";
    }
    else{
      print "Can't determine percentage of methylated Cs (not in CpG context) if value was 0\n";
    }
    ### calculating methylated CpG percentage if applicable
    if ($percent_meCpG){
      print "C methylated in CpG context:\t${percent_meCpG}%\n";
    }
    else{
      print "Can't determine percentage of methylated Cs (in CpG context) if value was 0\n";
    }
    print "\n\n";
  }
}

sub print_individual_C_methylation_states_paired_end_files{
  my ($meth_call,$chrom,$start,$id,$strand,$filehandle_index) = @_;
  my @methylation_calls = split(//,$meth_call);
  ############################################################
  ### . for bases not involving cytosines                  ###
  ### C for methylated C (was protected)                   ###
  ### c for not methylated C (was converted)               ###
  ### Z for methylated C in CpG context (was protected)    ###
  ### z for not methylated C in CpG context (was converted)###
  ############################################################
  my @match =();
  my $methyl_C_count = 0;
  my $methyl_CpG_count = 0;
  my $unmethylated_C_count = 0;
  my $unmethylated_CpG_count = 0;

  if ($strand eq '+'){
    for my $index (0..$#methylation_calls) {
      if ($methylation_calls[$index] eq 'C'){
	$counting{total_meC_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'+',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'c') {
	$counting{total_unmethylated_C_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'-',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'Z') {
	$counting{total_meCpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'+',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'z') {
	$counting{total_unmethylated_CpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'-',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
    }
  }
  elsif($strand eq '-'){
    for my $index (0..$#methylation_calls) {
      if ($methylation_calls[$index] eq 'C'){
	$counting{total_meC_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'+',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'c') {
	$counting{total_unmethylated_C_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'-',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'Z') {
	$counting{total_meCpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'+',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'z') {
	$counting{total_unmethylated_CpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'-',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
    }
  }
  else{
    die "This cannot happen $!\n";
  }
}


sub print_individual_C_methylation_states_single_end{

  my ($meth_call,$chrom,$start,$id,$seq,$strand,$filehandle_index) = @_;
  my @methylation_calls = split(//,$meth_call);
  ############################################################
  ### . for bases not involving cytosines                  ###
  ### C for methylated C (was protected)                   ###
  ### c for not methylated C (was converted)               ###
  ### Z for methylated C in CpG context (was protected)    ###
  ### z for not methylated C in CpG context (was converted)###
  ############################################################
  my @match =();
  my $methyl_C_count = 0;
  my $methyl_CpG_count = 0;
  my $unmethylated_C_count = 0;
  my $unmethylated_CpG_count = 0;

  if ($strand eq '+'){
    $start +=1;
    for my $index (0..$#methylation_calls) {
      ### methylated Cs (any context) will receive a forward (+) orientation
      ### not methylated Cs (any context) will receive a reverse (-) orientation
      if ($methylation_calls[$index] eq 'C'){
	$counting{total_meC_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'+',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'c') {
	$counting{total_unmethylated_C_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'-',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'Z') {
	$counting{total_meCpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'+',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'z') {
	$counting{total_unmethylated_CpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'-',$chrom,$start+$index,$methylation_calls[$index]),"\n";
      }
    }
  }
  elsif($strand eq '-'){
    $start += length($seq);
    for my $index (0..$#methylation_calls) {
      ### methylated Cs (any context) will receive a forward (+) orientation
      ### not methylated Cs (any context) will receive a reverse (-) orientation
      if ($methylation_calls[$index] eq 'C'){
	$counting{total_meC_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'+',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'c') {
	$counting{total_unmethylated_C_count}++;
	print {$fhs{$filehandle_index}->{other_c}} join ("\t",$id,'-',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'Z') {
	$counting{total_meCpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'+',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
      elsif ($methylation_calls[$index] eq 'z') {
	$counting{total_unmethylated_CpG_count}++;
	print {$fhs{$filehandle_index}->{CpG}} join ("\t",$id,'-',$chrom,$start-$index,$methylation_calls[$index]),"\n";
      }
    }
  }
  else{
    die "This cannot happen (or it shouldn't....$!\n";
  }
}



sub print_fastA_file_with_genomic_equivalent_sequences_from_paired_end_result_file{
  my ($id,$chrom,$start_read_1,$seq_1,$end_read_2,$seq_2) = @_;
  ### printing out the genomic equivalent sequences of the bisulfite reads (as these can't be aligned against repeats for example)
  print FASTA ">$id,$chrom,$start_read_1\n";
  print FASTA "$seq_1\n";
  print FASTA ">$id,$chrom,$end_read_2\n";
  print FASTA "$seq_2\n";
}


sub print_helpfile{

 print << 'HOW_TO';


DESCRIPTION


The following is a brief description of command line options and arguments to control the 


USAGE: Strand_specific_C_methylation.pl [options] <filenames>

  The methylation call string looks like this:
  ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
  ~~~   .   for bases not involving cytosines                   ~~~
  ~~~   C   for methylated C (was protected)                    ~~~
  ~~~   c   for not methylated C (was converted)                ~~~
  ~~~   Z   for methylated C in CpG context (was protected)     ~~~
  ~~~   z   for not methylated C in CpG context (was converted) ~~~
  ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~


ARGUMENTS:

<filenames>              A space-separated list of result files in Bismark format from 
                         which methylation information is extracted for every cytosine in 
                         the read.

OPTIONS:

-s/--single-end          Input file(s) are Bismark result file(s) generated from single-end
                         read data. Specifying either --single-end or --paired-end is
                         mandatory.

-p/--paired-end          Input file(s) are Bismark result file(s) generated from paired-end
                         read data. Specifying either --paired-end or --single-end is
                         mandatory.

--fasta                  Chosing this option will print out the genomic sequences that
                         correspond to the bisulfite mapped reads in FastA format.
                         This might be useful for certain applications where the
                         bisulfite read cannot be used (such as repeat analyses).

--ignore <int>           Ignore the first <int> bp when processing the methylation call
                         string. (As all reads are sorted in a forward direction this can
                         remove e.g. a restriction enzyme site at the start of each read).

Other:

-h/--help                Displays this help file and exits.


This script was last edited on 26 May 2010.

HOW_TO
}
