#!/usr/bin/perl --
use strict;
use warnings;
use Getopt::Long;


### This script is supposed to remove alignments to the same position in the genome which can arise by e.g. PCR amplification
### Paired-end alignments are considered a duplicate if both partner reasd start and end at the exact same position

### Note that this is not recommended for RRBS-type experiments!
### Added an automated report file Last modified: 11 Jan 2013


my $help;
my $representative;
my $single;
my $paired;
my $vanilla;
my $samtools_path;
my $bam;


my $command_line = GetOptions ('help' => \$help,
			       'representative' => \$representative,
			       's|single' => \$single,
			       'p|paired' => \$paired,
			       'vanilla' => \$vanilla,
			       'samtools_path=s' => \$samtools_path,
			       'bam' => \$bam,
			      );

die "Please respecify command line options\n\n" unless ($command_line);

if ($help){
  print_helpfile();
  exit;
}

my @filenames = @ARGV;

unless (@filenames){
  print "Please provide one or more Bismark output files for deduplication\n\n";
  sleep (2);
  print_helpfile();
  exit;
}

### OPTIONS
unless ($single or $paired){
  die "\nPlease select either -s to de-duplicate single-end or -p for paired-end Bismark files (--help for more info)\n\n";
}

if ($paired){
  if ($single){
    die "Please select either -s for single end files or -p for paired end files, but not both at the same time!\n\n";
  }
  if ($vanilla){
    warn "Processing paired-end custom Bismark output file(s):\n";
    warn join ("\t",@filenames),"\n\n";
  }
  else{
    warn "Processing paired-end Bismark output file(s) (SAM format):\n";
    warn join ("\t",@filenames),"\n\n";
  }
}
else{
  if ($vanilla){
    warn "Processing single-end custom Bismark output file(s):\n";
    warn join ("\t",@filenames),"\n\n";
  }
  else{
    warn "Processing single-end Bismark output file(s) (SAM format):\n";
    warn join ("\t",@filenames),"\n\n";
  }
}

### PATH TO SAMTOOLS
if (defined $samtools_path){
  # if Samtools was specified as full command
  if ($samtools_path =~ /samtools$/){
    if (-e $samtools_path){
      # Samtools executable found
    }
    else{
      die "Could not find an installation of Samtools at the location $samtools_path. Please respecify\n";
    }
  }
  else{
    unless ($samtools_path =~ /\/$/){
      $samtools_path =~ s/$/\//;
    }
    $samtools_path .= 'samtools';
    if (-e $samtools_path){
      # Samtools executable found
    }
    else{
      die "Could not find an installation of Samtools at the location $samtools_path. Please respecify\n";
    }
  }
}
# Check whether Samtools is in the PATH if no path was supplied by the user
else{
  if (!system "which samtools >/dev/null 2>&1"){ # STDOUT is binned, STDERR is redirected to STDOUT. Returns 0 if Samtools is in the PATH
    $samtools_path = `which samtools`;
    chomp $samtools_path;
  }
}
if ($bam){
  if (defined $samtools_path){
    $bam = 1;
  }
  else{
    warn "No Samtools found on your system, writing out a gzipped SAM file instead\n";
    $bam = 2;
  }
}


if ($representative){
  warn "\nIf there are several alignments to a single position in the genome the alignment with the most representative methylation call will be chosen (this might be the most highly amplified PCR product...)\n\n";
  sleep (2);
}
else{ # default; random (=first) alignment
  warn "\nIf there are several alignments to a single position in the genome the first alignment will be chosen. Since the input files are not in any way sorted this is a near-enough random selection of reads.\n\n";
  sleep (2);
}

foreach my $file (@filenames){

  ### writing to a report file
  my $report = $file;

  $report =~ s/\.gz$//;
  $report =~ s/\.sam$//;
  $report =~ s/\.bam$//;
  $report =~ s/\.txt$//;
  $report =~ s/$/.deduplication_report.txt/;

  open (REPORT,'>',$report) or die "Failed to write to report file to $report: S!\n\n";


  ### for representative methylation calls we need to discriminate between single-end and paired-end files as the latter have 2 methylation call strings
  if($representative){
    deduplicate_representative($file);
  }

  ### as the default option we simply write out the first read for a position and discard all others. This is the fastest option
  else{

    my %unique_seqs;
    my %positions;

    if ($file =~ /\.gz$/){
      open (IN,"zcat $file |") or die "Unable to read from gzipped file $file: $!\n";
    }
    elsif ($file =~ /\.bam$/){
      open (IN,"samtools view -h $file |") or die "Unable to read from BAM file $file: $!\n";
    }
    else{
      open (IN,$file) or die "Unable to read from $file: $!\n";
    }

    my $outfile = $file;
    $outfile =~ s/\.gz$//;
    $outfile =~ s/\.sam$//;
    $outfile =~ s/\.bam$//;
    $outfile =~ s/\.txt$//;

    if ($vanilla){
      $outfile =~ s/$/_deduplicated.txt/;
    }
    else{
      if ($bam == 1){
	$outfile =~ s/$/.deduplicated.bam/;
      }
      elsif ($bam == 2){
	$outfile =~ s/$/.deduplicated.sam.gz/;
      }
      else{
	$outfile =~ s/$/.deduplicated.sam/;
      }
    }
    if ($bam == 1){
      open (OUT,"| $samtools_path view -bSh 2>/dev/null - > $outfile") or die "Failed to write to $outfile: $!\n";
    }
    elsif($bam == 2){ ### no Samtools found on system. Using GZIP compression instead
      open (OUT,"| gzip -c - > $outfile") or die "Failed to write to $outfile: $!\n";
    }

    else{
      open (OUT,'>',$outfile) or die "Unable to write to $outfile: $!\n";
    }

    ### need to proceed slightly differently for the custom Bismark and Bismark SAM output
    if ($vanilla){
      $_ = <IN>; # Bismark version header
      print OUT; # Printing the Bismark version to the de-duplicated file again
    }
    my $count = 0;
    my $unique_seqs = 0;
    my $removed = 0;

    while (<IN>){

      if ($count == 0){
	if ($_ =~ /^Bismark version:/){
	  warn "The file appears to be in the custom Bismark and not SAM format. Please see option --vanilla!\n";
	  sleep (2);
	  print_helpfile();
	  exit;
	}
      }

      ### if this was a SAM file we ignore header lines
      unless ($vanilla){
	if (/^\@\w{2}\t/){
	  warn "skipping SAM header line:\t$_";
	  print OUT; # Printing the header lines again into the de-duplicated file
	  next;
	}
      }

      ++$count;

      my ($strand,$chr,$start,$end);
      my $line1;

      if ($vanilla){
	($strand,$chr,$start,$end) = (split (/\t/))[1,2,3,4];
      }
      else{ # SAM format
	($strand,$chr,$start,my $seq) = (split (/\t/))[1,2,3,9]; # we are assigning the FLAG value to $strand
	### SAM single-end
	if ($single){
	  $end = $start + length($seq) - 1;
	}
	elsif($paired){
	
	  ### storing the current line
	  $line1 = $_;

	  ### reading in the next line
	  $_ = <IN>;
	  # the only thing we need is the end position
	  my ($pos,$seq_2) = (split (/\t/))[3,9];
	  $end = $pos + length($seq_2) - 1;
	}
	else{
	  die "Input must be single or paired-end\n";
	}
      }

      my $composite = join (":",$strand,$chr,$start,$end);

      if (exists $unique_seqs{$composite}){
	++$removed;
	unless (exists $positions{$composite}){
	  $positions{$composite}++;
	}
      }
      else{
	if ($paired){
	  unless ($vanilla){
	    print OUT $line1; # printing first paired-end line for SAM output
	  }
	}
	print OUT; # printing single-end SAM alignment or second paired-end line
	$unique_seqs{$composite}++;
      }
    }

    my $percentage = sprintf("%.2f",$removed/$count*100);

    warn "\nTotal number of alignments analysed in $file:\t$count\n";
    warn "Total number duplicated alignments removed:\t$removed ($percentage %)\n";
    warn "Duplicated alignments were found at:\t",scalar keys %positions," different position(s)\n";

    print REPORT "Total number of alignments analysed in $file:\t$count\n";
    print REPORT "Total number duplicated alignments removed:\t$removed ($percentage %)\n";
    print REPORT "Duplicated alignments were found at:\t",scalar keys %positions," different position(s)\n";

  }
}

sub print_helpfile{
  print "\n",'='x111,"\n";
  print "\nThis script is supposed to remove alignments to the same position in the genome from the Bismark mapping output\n(both single and paired-end SAM files), which can arise by e.g. excessive PCR amplification. If sequences align\nto the same genomic position but on different strands they will be scored individually.\n\nNote that deduplication is not recommended for RRBS-type experiments!\n\nIn the default mode, the first alignment to a given position will be used irrespective of its methylation call\n(this is the fastest option, and as the alignments are not ordered in any way this is also near enough random).\n\n";
  print "This script expects the Bismark output to be in SAM format (Bismark v0.6.x or higher). To deduplicate the old\ncustom Bismark output please specify --vanilla\n\n";
  print '='x111,"\n\n";
  print ">>> USAGE: ./deduplicate_bismark_alignment_output.pl [options] filename(s) <<<\n\n";

  print "-s/--single\t\tdeduplicate single-end Bismark files (default format: SAM)\n";
  print "-p/--paired\t\tdeduplicate paired-end Bismark files (default format: SAM)\n";
  print "--vanilla\t\tThe input file is in the old custom Bismark format and not in SAM format\n";
  print "--representative\twill browse through all sequences and print out the sequence with the most representative\n                        (as in most frequent) methylation call for any given position. Note that this is very likely\n                        the most highly amplified PCR product for a given sequence\n\n";
  print "--bam\t\t\tThe output will be written out in BAM format instead of the default SAM format. This script will\n\t\t\tattempt to use the path to Samtools that was specified with '--samtools_path', or, if it hasn't\n\t\t\tbeen specified, attempt to find Samtools in the PATH. If no installation of Samtools can be found,\n\t\t\tthe SAM output will be compressed with GZIP instead (yielding a .sam.gz output file).\n";
  print "--samtools_path\t\tThe path to your Samtools installation, e.g. /home/user/samtools/. Does not need to be specified\n\t\t\texplicitly if Samtools is in the PATH already\n";
  print '='x111,"\n\n";

  print "This script was last modified on April 16, 2013\n\n";
}


sub deduplicate_representative {
  my $file = shift;

  my %positions;
  my %unique_seqs;

  my $count = 0;
  my $unique_seqs = 0;
  my $removed = 0;

  ### going through the file first and storing all positions as well as their methylation call strings in a hash
  if ($file =~ /\.gz$/){
    open (IN,"zcat $file |") or die "Unable to read from gzipped file $file: $!\n";
  }
  elsif ($file =~ /\.bam$/){
    open (IN,"samtools view -h $file |") or die "Unable to read from BAM file $file: $!\n";
  }
  else{
    open (IN,$file) or die "Unable to read from $file: $!\n";
  }

  if ($single){

    my $outfile = $file;
    $outfile =~ s/\.gz$//;
    $outfile =~ s/\.sam$//;
    $outfile =~ s/\.bam$//;
    $outfile =~ s/\.txt$//;

    if ($vanilla){
      $outfile =~ s/$/.deduplicated_to_representative_sequences.txt/;
    }
    else{
      if ($bam == 1){
	$outfile =~ s/$/.deduplicated_to_representative_sequences.bam/;
      }
      elsif ($bam == 2){
	$outfile =~ s/$/.deduplicated_to_representative_sequences.sam.gz/;
      }
      else{
	$outfile =~ s/$/.deduplicated_to_representative_sequences.sam/;
      }
    }

    if ($bam == 1){
      open (OUT,"| $samtools_path view -bSh 2>/dev/null - > $outfile") or die "Failed to write to $outfile: $!\n";
    }
    elsif($bam == 2){ ### no Samtools found on system. Using GZIP compression instead
      open (OUT,"| gzip -c - > $outfile") or die "Failed to write to $outfile: $!\n";
    }
    else{
      open (OUT,'>',$outfile) or die "Unable to write to $outfile: $!\n";
    }

    warn "Reading and storing all alignment positions\n";

    ### need to proceed slightly differently for the custom Bismark and Bismark SAM output
    if ($vanilla){
      $_ = <IN>; # Bismark version header
      print OUT; # Printing the Bismark version to the de-duplicated file again
    }

    while (<IN>){

      if ($count == 0){
	if ($_ =~ /^Bismark version:/){
	  warn "The file appears to be in the custom Bismark and not SAM format. Please see option --vanilla!\n";
	  sleep (2);
	  print_helpfile();
	  exit;
	}
      }

      ### if this was a SAM file we ignore header lines
      unless ($vanilla){
	if (/^\@\w{2}\t/){
	  warn "skipping SAM header line:\t$_";
	  print OUT; # Printing the header lines again into the de-duplicated file
	  next;
	}
      }

      my ($strand,$chr,$start,$end,$meth_call);

      if ($vanilla){
	($strand,$chr,$start,$end,$meth_call) = (split (/\t/))[1,2,3,4,7];
      }
      else{ # SAM format

	($strand,$chr,$start,my $seq,$meth_call) = (split (/\t/))[1,2,3,9,13]; # we are assigning the FLAG value to $strand
	### SAM single-end
	$end = $start + length($seq) - 1;
      }

      my $composite = join (":",$strand,$chr,$start,$end);

      $count++;
      $positions{$composite}->{$meth_call}->{count}++;
      $positions{$composite}->{$meth_call}->{alignment} = $_;
    }
    warn "Stored ",scalar keys %positions," different positions for $count sequences in total (+ and - alignments to the same position are scored individually)\n\n";
    close IN or die $!;
  }

  elsif ($paired){

    ### we are going to concatenate both methylation call strings from the paired end file to form a joint methylation call string

    my $outfile = $file;
    if ($vanilla){
      $outfile =~ s/$/_deduplicated_to_representative_sequences_pe.txt/;
    }
    else{
      $outfile =~ s/$/_deduplicated_to_representative_sequences_pe.sam/;
    }

    open (OUT,'>',$outfile) or die "Unable to write to $outfile: $!\n";
    warn "Reading and storing all alignment positions\n";

    ### need to proceed slightly differently for the custom Bismark and Bismark SAM output
    if ($vanilla){
      $_ = <IN>; # Bismark version header
      print OUT; # Printing the Bismark version to the de-duplicated file again
    }

    while (<IN>){

      if ($count == 0){
	if ($_ =~ /^Bismark version:/){
	  warn "The file appears to be in the custom Bismark and not SAM format. Please see option --vanilla!\n";
	  sleep (2);
	  print_helpfile();
	  exit;
	}
      }

      ### if this was a SAM file we ignore header lines
      unless ($vanilla){
	if (/^\@\w{2}\t/){
	  warn "skipping SAM header line:\t$_";
	  print OUT; # Printing the header lines again into the de-duplicated file
	  next;
	}
      }

      my ($strand,$chr,$start,$end,$meth_call_1,$meth_call_2);
      my $line1;

      if ($vanilla){
	($strand,$chr,$start,$end,$meth_call_1,$meth_call_2) = (split (/\t/))[1,2,3,4,7,10];
      }
      else{ # SAM paired-end format
	
	($strand,$chr,$start,$meth_call_1) = (split (/\t/))[1,2,3,13]; # we are assigning the FLAG value to $strand
	
	### storing the first line (= read 1 alignment)	
	$line1 = $_;
	
	### reading in the next line
	$_ = <IN>;
	# we only need the end position and the methylation call
	(my $pos,my $seq_2,$meth_call_2) = (split (/\t/))[3,9,13];
	$end = $pos + length($seq_2) - 1;
      }

      my $composite = join (":",$strand,$chr,$start,$end);
      $count++;
      my $meth_call = $meth_call_1.$meth_call_2;

      $positions{$composite}->{$meth_call}->{count}++;
      if ($vanilla){
	$positions{$composite}->{$meth_call}->{alignment} = $_;
      }
      else{ # SAM PAIRED-END
	$positions{$composite}->{$meth_call}->{alignment_1} = $line1;
	$positions{$composite}->{$meth_call}->{alignment_2} = $_;
      }
    }
    warn "Stored ",scalar keys %positions," different positions for $count sequences in total (+ and - alignments to the same position are scored individually)\n\n";
    close IN or die $!;
  }

  ### PRINTING RESULTS

  ### Now going through all stored positions and printing out the methylation call which is most representative, i.e. the one which occurred most often
  warn "Now printing out alignments with the most representative methylation call(s)\n";

  foreach my $pos (keys %positions){
    foreach my $meth_call (sort { $positions{$pos}->{$b}->{count} <=> $positions{$pos}->{$a}->{count} }keys %{$positions{$pos}}){
      if ($paired){
	if ($vanilla){
	  print OUT $positions{$pos}->{$meth_call}->{alignment};
	}
	else{
	  print OUT $positions{$pos}->{$meth_call}->{alignment_1}; # SAM read 1
	  print OUT $positions{$pos}->{$meth_call}->{alignment_2}; # SAM read 2
	}
      }
      else{ # single-end
	print OUT $positions{$pos}->{$meth_call}->{alignment};
      }
      $unique_seqs++;
      last; ### exiting once we printed a sequence with the most frequent methylation call for a position
    }
  }

  my $percentage = sprintf ("%.2f",$unique_seqs*100/$count);
  close OUT or die $!;
  print "\nTotal number of alignments analysed in $file:\t$count\n";
  print "Total number of representative alignments printed from $file in total:\t$unique_seqs ($percentage%)\n\n";
}


