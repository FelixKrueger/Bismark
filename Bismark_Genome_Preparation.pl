#!/usr/bin/perl --
use strict;
use warnings;
use Cwd;
$|++;


## This program is Copyright (C) 2010, Felix Krueger (felix.krueger@bbsrc.ac.uk)

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

use Getopt::Long;
my $verbose;
my $help;
my $man;
my $yes_to_all;
my $path_to_bowtie;

GetOptions ('verbose' => \$verbose, 'help' => \$help,'man' => \$man,'yes_to_all' => \$yes_to_all, 'path_to_bowtie:s' => \$path_to_bowtie);


my $genome_folder = shift @ARGV; # mandatory
my $CT_dir;
my $GA_dir;

if ($help or $man){
  print_helpfile();
  exit;
}

my @filenames = create_bisulfite_genome_folders();
process_sequence_files ();
launch_bowtie_indexer();

sub launch_bowtie_indexer{
  print "Bismark Genome Preparation - Step III: Launching the Bowtie indexer\n";
  print "Please be aware that this process can - depending on genome size - take up to several hours!\n";
  sleep(5);
  unless ($path_to_bowtie){
    while (1){
      print "Please specify the path to your bowtie installation:\t";
      $path_to_bowtie = <STDIN>;
      chomp $path_to_bowtie;
      if ($path_to_bowtie =~ /\S+/){
	if (chdir $path_to_bowtie){
	  last;
	}
	else{
	  warn "There seems to be a problem with the path specified: $!\n";
	}
      }
    }
  }

  # append a trailing folder / if necessary
  unless ($path_to_bowtie =~ /\/$/){
    $path_to_bowtie =~ s/$/\//;
  }

  # append bowtie-build to the bowtie path so we can use it later
  $path_to_bowtie =~ s/$/bowtie-build/;

  $verbose and print "\n";

  ### Forking the program to run 2 instances of bowtie-build (= the bowtie indexer)
  my $pid = fork();

  # parent process
  if ($pid){
    sleep(1);
    chdir $CT_dir or die "Unable to change directory: $!\n";
    $verbose and warn "Preparing indexing of CT converted genome in $CT_dir\n";
    my @fasta_files = <*.fa>;
    my $file_list = join (',',@fasta_files);
    $verbose and print "Parent process: Starting to index C->T converted genome with the following command:\n\n";
    $verbose and print "$path_to_bowtie -f $file_list BS_CT\n\n";
    sleep (11);
    exec ("$path_to_bowtie -f $file_list BS_CT");
  }

  # child process
  elsif ($pid == 0){
    sleep(2);
    chdir $GA_dir or die "Unable to change directory: $!\n";
    $verbose and warn "Preparing indexing of GA converted genome in $GA_dir\n";
    my @fasta_files = <*.fa>;
    my $file_list = join (',',@fasta_files);
    $verbose and print "Child process: Starting to index G->A converted genome with the following command:\n\n";
    $verbose and print "$path_to_bowtie -f $file_list BS_GA\n\n";
    $verbose and print "(starting in 10 seconds)\n";
    sleep(10);
    exec ("$path_to_bowtie -f $file_list BS_GA");
  }

  # if the platform doesn't support the fork command we will run the indexing processes one after the other 
  else{
    print "Forking process was not successful, therefore performing the indexing sequentially instead\n";
    sleep(10);

    ### moving to CT genome folder
    $verbose and warn "Preparing to index CT converted genome in $CT_dir\n";
    chdir $CT_dir or die "Unable to change directory: $!\n";
    my @fasta_files = <*.fa>;
    my $file_list = join (',',@fasta_files);
    $verbose and print "$file_list\n\n";
    sleep(2);
    system ("$path_to_bowtie -f $file_list BS_CT");
    @fasta_files=();
    $file_list= '';

    ### moving to GA genome folder
    $verbose and warn "Preparing to index GA converted genome in $GA_dir\n";
    chdir $GA_dir or die "Unable to change directory: $!\n";
    @fasta_files = <*.fa>;
    $file_list = join (',',@fasta_files);
    $verbose and print "$file_list\n\n";
    sleep(2);
    exec ("$path_to_bowtie -f $file_list BS_GA");
  }
}


sub process_sequence_files {

  my ($total_CT_conversions,$total_GA_conversions) = (0,0);
  $verbose and print "Bismark Genome Preparation - Step II: Bisulfite conversion of reference genome\n\n";
  sleep (3);

  $verbose and print "conversions performed:\n";
  $verbose and print join("\t",'chromosome','C->T','G->A'),"\n";

  foreach my $filename(@filenames){

    ### Extract chromosome number and sequence
    my $chromosome_number = chromosome_number ($filename);
    my $sequence = read_genomic_sequence($filename);

    my $bisulfite_CT_conversion_filename = "$CT_dir/$filename";
    $bisulfite_CT_conversion_filename =~ s/fa$/CT_conversion.fa/;

    my $bisulfite_GA_conversion_filename = "$GA_dir/$filename";
    $bisulfite_GA_conversion_filename =~ s/fa$/GA_conversion.fa/;

    ### Writing the chromosome out into a C->T converted version (equals forward strand conversion)
    # $verbose and warn "The forward strand sequence of chromosome $chromosome_number is now converted into a C->T bisulfite-treated version\n";

    my $CT_sequence = $sequence;
    my $CT_transliterations_performed = ($CT_sequence =~ tr/C/T/); # converts all Cs into Ts
    $total_CT_conversions += $CT_transliterations_performed;

    # $verbose and warn "Writing output to file $bisulfite_CT_conversion_filename\n";
    open (CT_CONVERT,'>',$bisulfite_CT_conversion_filename) or die "Can't write to file $bisulfite_CT_conversion_filename: $!\n";
    print CT_CONVERT ">",$chromosome_number,"_CT_converted\n";
    my $pos = 0;
    while ($pos < length $CT_sequence){
      print CT_CONVERT substr($CT_sequence,$pos,50),"\n";
      $pos += 50;
    }
    close (CT_CONVERT) or die "Failed to close filehandle: $!\n";

    ### Writing the chromosome out in a G->A converted version of the forward strand (this is equivalent to reverse-
    ### complementing the forward strand and then C->T converting it)
    # $verbose and warn "The reverse strand sequence of chromosome $chromosome_number is now bisulfite-treated (G->A conversion of the forward strand)\n";

    my $GA_sequence = $sequence;
    my $GA_transliterations_performed = ($GA_sequence =~ tr/G/A/); # converts all Gs to As on the forward strand
    $total_GA_conversions += $GA_transliterations_performed;

    # $verbose and warn "Writing output to file $bisulfite_GA_conversion_filename\n";
    open (GA_CONVERT,'>',$bisulfite_GA_conversion_filename) or die "Can't write to file $bisulfite_GA_conversion_filename: $!\n";
    print GA_CONVERT ">",$chromosome_number,"_GA_converted\n";
    $pos = 0;
    while ($pos < length $GA_sequence){
      print GA_CONVERT substr($GA_sequence,$pos,50),"\n";
      $pos += 50;
    }
    close (GA_CONVERT) or die "Failed to close filehandle: $!\n";
    $verbose and print join ("\t",$chromosome_number,$CT_transliterations_performed,$GA_transliterations_performed),"\n";
  }
  print "\nTotal number of conversions performed:\n";
  print "C->T:\t$total_CT_conversions\n";
  print "G->A:\t$total_GA_conversions\n";

  warn "\nStep II - Genome bisulfite conversions - completed\n\n\n";
}

sub read_genomic_sequence {
  my $filename = shift;
  my $sequence;
  # $verbose and warn "Now reading in sequence data from $filename\n";
  open (IN,$filename) or die "Can't open $filename: $!";
  $_ = <IN>; #removing FastA header
  while (<IN>){
    chomp;
    $sequence .= uc$_;
  }
  # $verbose and print "The length of the sequence was ",length$sequence," bp\n";
  return $sequence;
}

sub chromosome_number{
  my $filename = shift;
  if ($filename =~ /\.([^\.]+)\.fa$/){
    return $1;
  }
  else{
    die "Unable to extract chromosome number from $filename";
  }
}

sub create_bisulfite_genome_folders{

  $verbose and print "Bismark Genome Preparation - Step I: Preparing folders\n\n";
  # Ensuring a genome folder has been specified
  if ($genome_folder){
    unless ($genome_folder =~ /\/$/){
      $genome_folder =~ s/$/\//;
    }
    $verbose and print "Path to genome folder specified: $genome_folder\n";
    chdir $genome_folder or die "Could't move to directory $genome_folder. Make sure the directory exists! $!";
  }
  else{
    $verbose and print "Genome folder was not provided as argument ";
    while (1){
      print "Please specify a genome folder to be bisulfite converted:\n";
      $genome_folder = <STDIN>;
      chomp $genome_folder;

      # adding a trailing slash unless already present
      unless ($genome_folder =~ /\/$/){
	$genome_folder =~ s/$/\//;
      }
      if (chdir $genome_folder){
	last;
      }
      else{
	warn "Could't move to directory $genome_folder! $!";
      }
    }
  }
  if ($path_to_bowtie){
    unless ($path_to_bowtie =~ /\/$/){
      $path_to_bowtie =~ s/$/\//;
    }
    if (chdir $path_to_bowtie){
      $verbose and print "Path to bowtie specified: $path_to_bowtie\n";
    }
    else{
      $path_to_bowtie = '';
      print "There was an error with the path to bowtie: $!. You will be prompted again later\n";
    }
  }

  chdir $genome_folder or die "Could't move to directory $genome_folder. Make sure the directory exists! $!";
  # Exiting unless there are fastA files in the folder
  my @filenames = <*.fa>;
  die "The specified genome folder ($genome_folder) doesn't seem to contain any sequence files in fastA format!\n" unless (@filenames);

  # creating a directory inside the genome folder to store the bisfulfite genomes unless it already exists
  my $bisulfite_dir = "${genome_folder}Bisulfite_Genome/";
  unless (-d $bisulfite_dir){
    mkdir $bisulfite_dir or die "Unable to create directory $bisulfite_dir $!\n";
    $verbose and print "Created Bisulfite Genome folder $bisulfite_dir\n";
  }
  else{
    unless ($yes_to_all){
      while (1){
	print "\nA directory called $bisulfite_dir already exists.\nDo you want to overwrite the directory and all of its contents? (yes/no)\t";
	
	my $proceed = <STDIN>;
	chomp $proceed;
	if (lc$proceed eq 'yes' or lc$proceed eq 'y'){
	  last;
	}
	elsif (lc$proceed eq 'no' or lc$proceed eq 'n'){
	  die "Terminated by user\n\n";
	}
      }
    }
  }

  chdir $bisulfite_dir or die "Unable to move to $bisulfite_dir\n";
  $CT_dir = "${bisulfite_dir}CT_conversion/";
  $GA_dir = "${bisulfite_dir}GA_conversion/";

  # creating 2 subdirectories to store a C->T (forward strand conversion) and a G->A (reverse strand conversion)
  # converted version of the genome
  unless (-d $CT_dir){
    mkdir $CT_dir or die "Unable to create directory $CT_dir $!\n";
    $verbose and print "Created Bisulfite Genome folder $CT_dir\n";
  }
  unless (-d $GA_dir){
    mkdir $GA_dir or die "Unable to create directory $GA_dir $!\n";
    $verbose and print "Created Bisulfite Genome folder $GA_dir\n";
  }

  # moving back to the original genome folder
  chdir $genome_folder or die "Could't move to directory $genome_folder $!";
  # $verbose and print "Moved back to genome folder folder $genome_folder\n";
  warn "\nStep I - Prepare genome folders - completed\n\n\n";
  return @filenames;
}

sub print_helpfile{
  print << 'HOW_TO';


DESCRIPTION

This script is supposed to convert a specified reference genome into two different bisulfite
converted versions and index them for alignments with Bowtie. One bisulfite genome will have 
all Cs converted to Ts (C->T), and the other will have all Gs converted to As (G->A). Both
bisulfite genomes will be stored in subfolders within the reference genome folder. Once the 
bisulfite conversion has been completed the program will fork and launch two simultaneous
versions of the bowtie indexer (bowtie-build). Be aware that the indexing process can take
up to several hours; this will mainly depend on genome size and system resources.




The following is a brief description of command line options and arguments to control the
Bismark Genome Preparation script:


USAGE: Bismark_Genome_Preparation.pl [options] <arguments>


OPTIONS:

[--help/--man]           Displays this help file.

[--verbose]              Print verbose output for more details or debugging.

[--yes/--yes_to_all]     Answer yes to safety related questions (such as "Are you sure you
                         want to overwrite any existing folder called Bisulfite_Genomes?").

[--path_to_bowtie]       The full path to the bowtie installation on your system. If the path 
</../../>                is not provided as an option you will be prompted for it later.


ARGUMENTS:

<path_to_genome_folder>  The full path to the folder containing the genome to be bisulfite
                         converted. At the current time Bismark_Genome_Preparation expects
                         one or more fastA files in the folder (with the file extension: .fa).
                         If the path is not provided as an argument you will be prompted for it.



This script was last edited on 18 May 2010.

HOW_TO
}
