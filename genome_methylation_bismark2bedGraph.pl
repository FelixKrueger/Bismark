#!/usr/bin/perl -w

# Count methylation of cytosine genome-wide from Bismark methylation caller output and generates a bedGraph

# Script origninally created by O. Tam, modified by F. Krueger on 16 Mar 2011.
# Corrected read coordinates to being 0 based, inspired by Timothy Hughes, 08 June 2011
# Bug with 0-based coordinates fixed by Michael A. Bentley, 13 Sep 2011

use strict;
use warnings;
use Carp;
use Getopt::Long;

my $coverage_threshold = 1; # Minimum number of reads covering before calling methylation status
my $remove;
my $help;
my $split;
my $counts;

GetOptions("cutoff=i" => \$coverage_threshold,
	   "remove_spaces" => \$remove,
	   "h|help" => \$help,
	   "s|split_by_chromosome" => \$split,
	   "counts" => \$counts,
	  );
if ($help){
  die usage();
}

if(scalar @ARGV < 1){
  warn "Missing input file\n";
  die usage();
}

my $infile = shift @ARGV;

if ($remove){

  warn "\nNow replacing whitespaces in the sequence ID field in the Bismark mapping output prior to bedGraph conversion\n\n";

  open (IN,$infile) or die $!;

  my $removed_spaces_outfile = $infile;
  $removed_spaces_outfile =~ s/$/.spaces_removed.txt/;

  open (REM,'>',$removed_spaces_outfile) or die "Couldn't write to file $removed_spaces_outfile: $!\n";

  $_ = <IN>;
  print REM $_; ### Bismark version header

  while (<IN>){
    chomp;
    my ($id,$strand,$chr,$pos,$context) = (split (/\t/));
    $id =~ s/\s+/_/g;
    print REM join ("\t",$id,$strand,$chr,$pos,$context),"\n";
  }

  close IN or die $!;
  close REM or die $!;

  ### changing the infile name to the new outfile without spaces
  $infile = $removed_spaces_outfile;
}

my %fhs;
my @temp_files;
if ($split){
  warn "Now generating individual files for each chromosome (sorting very large files might fail otherwise...)\n";

  open (IN,$infile) or die $!;
  $_ = <IN>; ### Bismark version header

  while (<IN>){
    chomp;
    my ($chr) = (split (/\t/))[2];

    ### Replacing pipe characters ('|') in temporary filenames with underscores
    $chr =~ s/|/_/g;

    unless (exists $fhs{$chr}){
      open ($fhs{$chr},'>','chr'.$chr.'.meth_extractor.temp') or die "Failed to open filehandle: $!";
    }
    print {$fhs{$chr}} "$_\n";
  }

  warn "Finished writing out individual chromosome files\n";
  sleep (5);

  warn "Collecting temporary chromosome file information...\n";
  sleep (1);
  @temp_files = <*.meth_extractor.temp>;

  # warn join ("\n",@temp_files),"\n";
}
else{
  @temp_files = $infile;
}

my @methylcalls = qw (0 0 0); # [0] = methylated, [1] = unmethylated, [2] = total

warn "processing the following input file(s):\n";
warn join ("\n",@temp_files),"\n\n";
sleep (5);


foreach my $in (@temp_files){
  warn "Sorting input file $in by positions\n";
  open my $ifh, "sort -k3,3 -k4,4n $in |" or die "Input file could not be sorted. $!";
  # print "Chromosome\tStart Position\tEnd Position\tMethylation Percentage\n";

  ############################################# m.a.bentley - moved the variables out of the while loop to hold the current line data {

  my $name;
  my $meth_state;
  my $chr = "";
  my $pos = 0;
  my $meth_state2;

  my $last_pos;
  my $last_chr;

  #############################################  }

  while(my $line = <$ifh>){
    next if $line =~ /^Bismark/;
    chomp $line;

    ########################################### m.a.bentley - (1) set the last_chr and last_pos variables early in the while loop, before the line split (2) removed unnecessary setting of same variables in if statement {

    $last_chr = $chr;
    $last_pos = $pos;
    ($name, $meth_state, $chr, $pos, $meth_state2) = split "\t", $line;

    if(($last_pos ne $pos) || ($last_chr ne $chr)){
      generate_output($last_chr,$last_pos) if $methylcalls[2] > 0;
      @methylcalls = qw (0 0 0);
    }

    #############################################  }

    my $validated = validate_methylation_call($meth_state, $meth_state2);
    unless($validated){
      warn "Methylation state of sequence ($name) in file ($infile) on line $. is inconsistent (meth_state is $meth_state, meth_state2 = $meth_state2)\n";
      next;
    }
    if($meth_state eq "+"){
      $methylcalls[0]++;
      $methylcalls[2]++;
    }
    else{
      $methylcalls[1]++;
      $methylcalls[2]++;
    }
  }

  ############################################# m.a.bentley - set the last_chr and last_pos variables for the last line in the file (outside the while loop's scope using the method i've implemented) {

  $last_chr = $chr;
  $last_pos = $pos;
  if ($methylcalls[2] > 0){
    generate_output($last_chr,$last_pos) if $methylcalls[2] > 0;
  }
  #############################################  }

  close $ifh or die $!;

  @methylcalls = qw (0 0 0); # resetting @methylcalls

  if ($split){ # deleting temporary files
    my $delete = unlink $in;
    if ($delete){
      warn "Successfully deleted the temporary input file $in\n\n";
    }
    else{
      warn "The temporary inputfile $in could not be deleted $!\n\n";
    }
  }
}


sub generate_output{
  my $methcount = $methylcalls[0];
  my $nonmethcount = $methylcalls[1];
  my $totalcount = $methylcalls[2];
  my $last_chr = shift;
  my $last_pos = shift;
  croak "Should not be generating output if there's no reads to this region" unless $totalcount > 0;
  croak "Total counts ($totalcount) is not the sum of the methylated ($methcount) and unmethylated ($nonmethcount) counts" if $totalcount != ($methcount + $nonmethcount);

  ############################################# m.a.bentley - declare a new variable 'bed_pos' to distinguish from bismark positions (-1) - previous scripts modified the last_pos variable earlier in the script leading to problems in meth % calculation {

  my $bed_pos = $last_pos -1; ### Bismark coordinates are 1 based whereas bedGraph coordinates are 0 based.
  my $meth_percentage;
  ($totalcount >= $coverage_threshold) ? ($meth_percentage = ($methcount/$totalcount) * 100) : ($meth_percentage = undef);
  # $meth_percentage =~ s/(\.\d\d).+$/$1/ unless $meth_percentage =~ /^Below/;
  if (defined $meth_percentage){
    if ($counts){
      print "$last_chr\t$bed_pos\t$bed_pos\t$meth_percentage\t$methcount\t$nonmethcount\n";
    }
    else{
      print "$last_chr\t$bed_pos\t$bed_pos\t$meth_percentage\n";
    }
  }
  #############################################  }
}

sub validate_methylation_call{
  my $meth_state = shift;
  croak "Missing (+/-) methylation call" unless defined $meth_state;
  my $meth_state2 = shift;
  croak "Missing alphabetical methylation call" unless defined $meth_state2;
  my $is_consistent;
  ($meth_state2 =~ /^z/i) ? ($is_consistent = check_CpG_methylation_call($meth_state, $meth_state2)) 
                          : ($is_consistent = check_nonCpG_methylation_call($meth_state,$meth_state2));
  return 1 if $is_consistent;
  return 0;
}

sub check_CpG_methylation_call{
  my $meth1 = shift;
  my $meth2 = shift;
  return 1 if($meth1 eq "+" && $meth2 eq "Z");
  return 1 if($meth1 eq "-" && $meth2 eq "z");
  return 0;
}

sub check_nonCpG_methylation_call{
  my $meth1 = shift;
  my $meth2 = shift;
  return 1 if($meth1 eq "+" && $meth2 eq "C");
  return 1 if($meth1 eq "+" && $meth2 eq "X");
  return 1 if($meth1 eq "+" && $meth2 eq "H");
  return 1 if($meth1 eq "-" && $meth2 eq "c");
  return 1 if($meth1 eq "-" && $meth2 eq "x");
  return 1 if($meth1 eq "-" && $meth2 eq "h");
  return 0;
}

sub usage{
  print <<EOF

  Usage: genome_methylation_bismark2bedGraph.pl (--cutoff [threshold] ) [Bismark methylation caller output] > [output]

  --cutoff [threshold]    -  The minimum number of times a methylation state was
                             seen for that nucleotide before its methylation 
                             percentage is reported.
                             Default is no threshold

  --remove_spaces         -  Replaces whitespaces in the sequence ID field with underscores to allow sorting.

  --s/split_by_chromosome -  Splits methylation extractor output up into temporary files for each chromosome, and uses
                             these input files for sorting. Please keep in mind that this temprarily requires more space
                             on your hard disk!

  --counts                -  Adds two additional columns to the output file to enable further calculations:
                             col 5: methylated calls
                             col 6: unmethylated calls

  The output file is a tab-delimited bedGraph file with the following information:

  <Chromosome> <Start Position> <End Position> <Methylation Percentage>

  Please note that the option --counts adds 2 additional columns, so it is technically no longer in bedGraph format!

  Bismark methylation caller (v0.2.0 or later) should produce three output files
    (CpG, CHG and CHH) when using the "--comprehensive" option
    (Two files if using the "--merge_non_CpG" option).
    To count both CpG and Non-CpG, combine the output files.

  Bismark methylation caller (v0.1.5 or earlier) should produce two output files
    (CpG and Non-CpG) when using the "--comprehensive" option.
    To count both CpG and Non-CpG, combine the two output files.



                          Script last modified: 28 Feb 2013.

EOF
    ;
  exit 1;
}


