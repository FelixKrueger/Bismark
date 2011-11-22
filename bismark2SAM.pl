#!/usr/bin/perl -w
#
# Converts bismark mapping output to SAM format.
# Usage: bismark_to_SAM.pl (-p) (-s) -c [chrom sizes file] -i [bismark mapping output] -o [SAM output]
# Original script by O. Tam (2010), modified by C. Whelan

### version 4 and 5 reworked by F. Krueger
### The script does now handle and include read quality scores, and handles single-end non-directional
### datasets. Both directional and non-directional paired-end alignments are now handled correctly. SAM
### file sorting is now optional since BAM files can be sorted via samtools sort later on.

### version 5 adds different bitwise FLAG values to the SAM output so that alignments to the OT and CTOT
### and OB and CTOB strands are now flagging the strand they originated from better. Thanks to Enrique
### Vidal for his contributions to improving this issue.

### version 5_xm adds the methylation call as an additional field for easier downstream processing and also
### appends .2 to the read ID of a second read for paired-end alignments. Thanks to Tony McBryan for adding
### these features.

### version 6: If sequences are reverse complemented for SAM output, the quality and methylation call strings
### are now being reversed as well. Thanks to Peter Hickey for this suggestion. (22 Nov, 2011)


### Last edit: November 22, 2011


use strict;
use warnings;
use Getopt::Long;
use Carp;
my $version_info = '6 (Last edit: 22 Nov 2011)';

# chrom_sizes_file Contains chromosome name used in mapping (column 1) and its length (column 2)
my ($chrom_sizes_file,$bismark_infile,$sam_outfile,$is_paired_end,$sorting) = parse_cmd_line();

### Main program ###

verify_input_file();
generate_SAM_header();

if($is_paired_end){
  convert_pe_bismark_to_SAM();
}
else{
  convert_se_bismark_to_SAM();
}

### Subroutines ###

sub parse_cmd_line{
  my $help;
  my $version;
  my $chrom_sizes_file;
  my $bismark_infile;
  my $is_paired_end;
  my $sorting;
  my $sam_outfile;

  my $command_line = GetOptions ('help' => \$help,
				 'version' => \$version,
				 'p|paired_end' => \$is_paired_end,
				 's|sorting' => \$sorting,
				 'o|outfile=s' => \$sam_outfile,
				 'i|infile=s' => \$bismark_infile,
				 'c|chrom_file=s' => \$chrom_sizes_file,
				);

  ### EXIT ON ERROR if there were errors with any of the supplied options
  unless ($command_line){
    warn "Please respecify command line options\n";
    usage();
  }

  ### HELP OPTION
  if ($help){
    usage();
  }

  ### VERSION
  if ($version){
    die "\nbismark2SAM conversion script version:\t$version_info\n\t\tfor help type bismark2SAM.pl --help\n\n";
  }

  ### CHROMOSOME SIZES FILE
  die "Please specifify a chrom_sizes file (-c chrom_sizes_file)\n" unless defined $chrom_sizes_file;

  ### BISMARK INFILE
  die "Please specify a Bismark infile (-i infile)\n" unless defined $bismark_infile;

  ### PAIRED-END FLAG
  unless ($is_paired_end){
    $is_paired_end = 0;
  }

  ### SAM sorting
  ### sorts the SAM output by chromosome and start position, not really needed since samtools sort will sort anyway
  if ($sorting){
    warn "\nSAM output will be sorted by chromosome and start position\n";
  }
  else{
    $sorting = 0;
  }

  ### SAM OUTFILE
  $sam_outfile = $bismark_infile . ".sam" unless defined $sam_outfile;

  return ($chrom_sizes_file,$bismark_infile,$sam_outfile,$is_paired_end,$sorting);
}


sub verify_input_file{
  open my $ifh, "<", $bismark_infile or die "Cannot open input file ($bismark_infile). $!";
  my $line = <$ifh>;
  $line = <$ifh> if $line =~ /^Bismark version/;
  chomp $line;
  $line =~ s/\r//;

  my @NR = split "\t", $line;

  if($is_paired_end){
    die "Input file ($bismark_infile) is not a paired-end Bismark mapping result file\n" if scalar @NR != 15;
  }
  else{
    die "Input file ($bismark_infile) is not a single-end Bismark mapping result file (number of fields: ",scalar @NR,")\n" if scalar @NR != 11;
  }
  close $ifh or die $!;
}

sub generate_SAM_header{
  open my $ifh, "<", $chrom_sizes_file or die "Cannot open chrom_sizes file ($chrom_sizes_file). $!";
  open (OUT, ">", $sam_outfile) or die "Cannot write to output file ($sam_outfile): $!\n";
  print OUT "\@" . "HD\tVN:1.0\tSO:unsorted\n";          # @HD = header, VN = version, SO = sort order [technically sorted by chromosome and start coordinate]
  while (my $line = <$ifh>){
    chomp $line;
    $line =~ s/\r//; # deletes carriage return characters
    my ($chr, $length) = split "\t", $line;
    die "Chromosome length is not numeric in file ($chrom_sizes_file) on line $.\n" if $length =~ /\D/;
    print OUT "\@" . "SQ\tSN:$chr\tLN:$length\n";        # @SQ = sequence, SN = seq name, LN = length
  }
}

sub convert_se_bismark_to_SAM{
  my %seqs;

  open my $ifh, "<", $bismark_infile or die "Cannot open input file ($bismark_infile). $!";

  my $count = 0;

  while(my $line = <$ifh>){
    next if $line =~ /^Bismark version/; # first line
    ++$count;
    chomp $line;
    my ($id,$strand,$chr,$start,$stop,$actual_seq,$ref_seq,$methcall,$read_conversion,$genome_conversion,$qual) = (split ("\t", $line))[0,1,2,3,4,5,6,7,8,9,10];

    ### Input validations ###
    die "Strand information ($strand) is incorrect in file ($bismark_infile) on line $.\n" if $strand =~ /[^\+\-]/;
    die "Start position ($start) is not numeric in file ($bismark_infile) on line $.\n" if $start =~ /\D/;
    die "End position ($stop) is not numeric in file ($bismark_infile) on line $.\n" if $stop =~ /\D/;

    # Assumes bisulfite sequences has A,C,T,G and N only
    die "Bisulfite sequence ($actual_seq) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $actual_seq =~ /[^ACTGNactgn]/;

    # Allows all degenerate nucleotide sequences in reference genome
    die "Reference sequence ($ref_seq) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $ref_seq =~ /[^ACTGNRYMKSWBDHVactgnrymkswbdhv]/;


    ### This is a description of the bitwise FLAG field which needs to be set for the SAM file taken from: "The SAM Format Specification (v1.4-r985), September 7, 2011"
    ## FLAG: bitwise FLAG. Each bit is explained in the following table:

    ## Bit    Description                                                Comment                                Value
    ## 0x1    template having multiple segments in sequencing            0: single-end 1: paired end            value: 2^^0 (  1)
    ## 0x2    each segment properly aligned according to the aligner     true only for paired-end alignments    value: 2^^1 (  2)
    ## 0x4    segment unmapped                                           ---                                           ---
    ## 0x8    next segment in the template unmapped                      ---                                           ---
    ## 0x10   SEQ being reverse complemented                                                                    value: 2^^4 ( 16)
    ## 0x20   SEQ of the next segment in the template being reversed                                            value: 2^^5 ( 32)
    ## 0x40   the first segment in the template                          read 1                                 value: 2^^6 ( 64)
    ## 0x80   the last segment in the template                           read 2                                 value: 2^^7 (128)
    ## 0x100  secondary alignment                                        ---                                           ---
    ## 0x200  not passing quality controls                               ---                                           ---
    ## 0x400  PCR or optical duplicate                                   ---                                           ---


    my $flag;                                                 # FLAG variable used for SAM format.

    if ($strand eq "+"){
      if ($read_conversion eq 'CT' and $genome_conversion eq 'CT'){
	$flag = 0;                                            # 0 for "+" strand (OT)
      }
      elsif ($read_conversion eq 'GA' and $genome_conversion eq 'GA'){
	$flag = 16;                                           # 16 for "-" strand (CTOB, yields information for the original bottom strand)
      }
      else{
	die "Unexpected strand and read/genome conversion: strand: $strand, read conversion: $read_conversion, genome_conversion: $genome_conversion\n\n";
      }
    }
    elsif ($strand eq "-"){
      if ($read_conversion eq 'CT' and $genome_conversion eq 'GA'){
	$flag = 16;                                           # 16 for "-" strand (OB)
      }
      elsif ($read_conversion eq 'GA' and $genome_conversion eq 'CT'){
	$flag = 0;                                           # 0 for "+" strand (CTOT, yields information for the original top strand)
      }
      else{
	die "Unexpected strand and read/genome conversion: strand: $strand, read conversion: $read_conversion, genome_conversion: $genome_conversion\n\n";
      }
    }
    else{
      die "Unexpected strand information: $strand\n\n";
    }


    my $mapq = 255;                                           # Assume mapping quality is unavailable
    my $cigar = length($actual_seq) . "M";                    # Assume no indel during mapping (only matches and mismatches)
    my $mrnm = "*";                                           # Paired-end variable
    my $mpos = 0;                                             # Paired-end variable
    my $isize = 0;                                            # Paired-end variable
    $actual_seq = revcomp($actual_seq) if $strand eq "-";     # Sequence represented on the forward genomic strand

    if ($read_conversion eq 'CT'){
      $ref_seq = substr($ref_seq, 0, length($ref_seq) - 2);   # Removes additional nucleotides from the 3' end. This only works for the original top or bottom strands
    }
    else{
      $ref_seq = substr($ref_seq, 2, length($ref_seq) - 2);   # Removes additional nucleotides from the 5' end. This works for the complementary strands in non-directional libraries
    }

    if ($strand eq '-'){
      $ref_seq = revcomp($ref_seq);                           # Required for comparison with actual sequence
      $qual = reverse $qual;                                  # if the sequence was reverse-complemented the quality string needs to be reversed as well
    }

    my $hemming_dist = hemming_dist($actual_seq, $ref_seq);
    my $NM_tag = "NM:i:$hemming_dist";                        # Optional tag: edit distance based on nucleotide differences
    my $MD_tag = make_mismatch_string($actual_seq, $ref_seq); # Optional tag: String to provide mismatch and indel information. Expect only mismatches

    my $XM_tag;                                               # Optional tag: String to provide Methylation calls
    if ($strand eq '+'){
      $XM_tag = "XM:Z:$methcall";
    }
    elsif ($strand eq '-'){
      $XM_tag = 'XM:Z:'.reverse $methcall;                    # if the sequence was reverse-complemented the methylation call string needs to be reversed as well
    }
    else{
      die "strand was neither + or -: $strand \n";
    }

    # SAM format: QNAME, FLAG, RNAME, 1-based START, MAPQ, CIGAR, MRNM, MPOS, ISIZE, SEQ, QUAL

    if ($sorting){
      $seqs{$count} = {
		       id => $id,
		       flag => $flag,
		       chr => $chr,
		       start => $start,
		       mapq => $mapq,
		       cigar => $cigar,
		       mrnm => $mrnm,
		       mpos => $mpos,
		       isize => $isize,
		       actual_seq => $actual_seq,
		       qual => $qual,
		       NM_tag => $NM_tag,
		       MD_tag => $MD_tag,
		       XM_tag => $XM_tag,			
		      }
	;
    }
    ### this is the defaultline-by-line SAM output
    else{
      print OUT join("\t", ($id, $flag, $chr, $start, $mapq, $cigar, $mrnm, $mpos, $isize, $actual_seq, $qual, $NM_tag, $MD_tag, $XM_tag)), "\n";
    }
  }

  if ($sorting){
    foreach my $seq (sort {$seqs{$a}->{chr} cmp $seqs{$b}->{chr} || $seqs{$a}->{start}<=>$seqs{$b}->{start} } keys %seqs){
      print OUT join("\t", ($seqs{$seq}->{id}, $seqs{$seq}->{flag}, $seqs{$seq}->{chr}, $seqs{$seq}->{start}, $seqs{$seq}->{mapq}, $seqs{$seq}->{cigar},$seqs{$seq}->{mrnm}, $seqs{$seq}->{mpos}, $seqs{$seq}->{isize}, $seqs{$seq}->{actual_seq}, $seqs{$seq}->{qual}, $seqs{$seq}->{NM_tag}, $seqs{$seq}->{MD_tag}, $seqs{$seq}->{XM_tag})), "\n";
    }
  }
}

sub convert_pe_bismark_to_SAM{

  my %seqs1;
  my %seqs2;

  open my $ifh, "<", $bismark_infile or die "Cannot open input file ($bismark_infile). $!";

  my $count = 0;

  while(my $line = <$ifh>){
    next if $line =~ /^Bismark version/;
    chomp $line;

    $count++;

    my ($id,$strand,$chr,$start,$stop,$actual_seq1,$ref_seq1,$methcall1,$actual_seq2,$ref_seq2,$methcall2,$first_read_conversion,$genome_conversion,$qual1,$qual2) = (split ("\t", $line))[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14];

    my $id1 = $id;
    chop($id);
    my $id2 = $id."2";

    ### Input validations ###
    die "Strand information ($strand) is incorrect in file ($bismark_infile) on line $.\n" if $strand =~ /[^\+\-]/;
    die "Start position ($start) is not numeric in file ($bismark_infile) on line $.\n" if $start =~ /\D/;
    die "End position ($stop) is not numeric in file ($bismark_infile) on line $.\n" if $stop =~ /\D/;

    # Assumes bisulfite sequences has A,C,T,G and N only
    die "Bisulfite sequence ($actual_seq1) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $actual_seq1 =~ /[^ACTGNactgn]/;
    die "Bisulfite sequence ($actual_seq2) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $actual_seq2 =~ /[^ACTGNactgn]/;

    # Allows all degenerate nucleotide sequences in reference genome
    die "Reference sequence ($ref_seq1) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $ref_seq1 =~ /[^ACTGNRYMKSWBDHVactgnrymkswbdhv]/;
    die "Reference sequence ($ref_seq2) contains invalid nucleotides in file ($bismark_infile) on line $.\n" if $ref_seq2 =~ /[^ACTGNRYMKSWBDHVactgnrymkswbdhv]/;


    my $index; # used to store the srand origin of the alignment in a less convoluted way


    if ($first_read_conversion eq 'CT' and $genome_conversion eq 'CT'){
      $index = 0; ## this is OT   (original top strand)
    }	
    elsif ($first_read_conversion eq 'GA' and $genome_conversion eq 'CT'){
      $index = 1; ## this is CTOT (complementary to OT)
    }
    elsif ($first_read_conversion eq 'GA' and $genome_conversion eq 'GA'){
      $index = 2; ## this is CTOB (complementary to OB)
    }
    elsif ($first_read_conversion eq 'CT' and $genome_conversion eq 'GA'){
      $index = 3; ## this is OB   (original bottom)
    }
    else {
      die "Unexpected combination of read and genome conversion: $first_read_conversion / $genome_conversion\n";
    }
	
    ### we need to remove 2 bp of the genomic sequence as we were extracting read + 2bp long fragments to make a methylation call at the
    ### first or last position.
	
    if ($index == 0 or $index == 3){ # OT or OB
      $ref_seq1 = substr($ref_seq1,0,length($ref_seq1)-2);
      $ref_seq2 = substr($ref_seq2,2,length($ref_seq2)-2);
    }
    else{ # CTOT or CTOB
      $ref_seq1 = substr($ref_seq1,2,length($ref_seq1)-2);
      $ref_seq2 = substr($ref_seq2,0,length($ref_seq2)-2);
    }

    ### This is a description of the bitwise FLAG field which needs to be set for the SAM file taken from: "The SAM Format Specification (v1.4-r985), September 7, 2011"
    ## FLAG: bitwise FLAG. Each bit is explained in the following table:

    ## Bit    Description                                                Comment                                Value
    ## 0x1    template having multiple segments in sequencing            0: single-end 1: paired end            value: 2^^0 (  1)
    ## 0x2    each segment properly aligned according to the aligner     true only for paired-end alignments    value: 2^^1 (  2)
    ## 0x4    segment unmapped                                           ---                                           ---
    ## 0x8    next segment in the template unmapped                      ---                                           ---
    ## 0x10   SEQ being reverse complemented                                                                    value: 2^^4 ( 16)
    ## 0x20   SEQ of the next segment in the template being reversed                                            value: 2^^5 ( 32)
    ## 0x40   the first segment in the template                          read 1                                 value: 2^^6 ( 64)
    ## 0x80   the last segment in the template                           read 2                                 value: 2^^7 (128)
    ## 0x100  secondary alignment                                        ---                                           ---
    ## 0x200  not passing quality controls                               ---                                           ---
    ## 0x400  PCR or optical duplicate                                   ---                                           ---


    ### Generate output for Read 1
    my $flag1;                                                    # FLAG variable used for SAM format.

    ### These FLAGS take the strand identity into account, and were contributed by Enrique Vidal (13 Sept 2011)
    ($strand eq "+") ? ($flag1 = 67) : ($flag1 = 115);            #  67 if read 1 is positive strand (1+2+64)
                                                                  # 115 if read 1 is negative strand (1+2+16+32+64)

    my $read1start;
    ($strand eq "+") ? ($read1start = $start) : ($read1start = $stop - length($actual_seq1) + 1);

    my $mapq = 255;                                               # Assume mapping quality is unavailable
    my $cigar1 = length($actual_seq1) . "M";                      # Assume no indel during mapping (only matches and mismatches)
    my $mrnm = $chr;                                              # Chromosome of mate
    my $mpos1;       # Left-most position of mate
    ($strand eq '+') ? ($mpos1 = $stop - length($actual_seq2)+1) : ($mpos1 = $start);
    my $isize1;
    ($strand eq '+') ? ($isize1 = $stop - $start) : ($isize1 = $start - $stop);

    if ($strand eq '-'){
      $actual_seq1 = revcomp($actual_seq1);                       # Sequence represented on the forward genomic strand
      $ref_seq1 = revcomp($ref_seq1);                             # Required for comparison with actual sequence
      $qual1 = reverse $qual1;                                    # we need to reverse the quality string as well
    }

    my $hemming_dist = hemming_dist($actual_seq1,$ref_seq1);
    my $NM_tag1 = "NM:i:$hemming_dist";                           # Optional tag: edit distance based on nucleotide differences
    my $MD_tag1 = make_mismatch_string($actual_seq1,$ref_seq1);   # Optional tag: String to provide mismatch and indel information. Expect only mismatches

    my $XM_tag1;                                                  # Optional tag: String to provide Methylation calls
    if ($strand eq '-'){
      $XM_tag1 = 'XM:Z:'.reverse $methcall1;                      # Needs to be reversed if the sequence was reverse complemented
    }
    elsif ($strand eq '+'){
      $XM_tag1 = "XM:Z:$methcall1";
    }
    else{
      die "Strand was neither + nor - but: $strand\n";
    }


    ### Generate output for Read 2
    my $flag2;                                                    # FLAG variable used for SAM format.
    # ($strand eq "+") ? ($flag2 = 147) : ($flag2 = 163);         # 147 if read 1 is positive strand, 163 if read 1 is negative strand  ### OLD VERSION

    ### These FLAGS take the strand identity into account, and were contributed by Enrique Vidal (13 Sept 2011)
    ($strand eq "+") ? ($flag2 = 131) : ($flag2 = 179);           # 131 if read 1 is positive strand (1+2+128)
                                                                  # 179 if read 1 is negative strand (1+2+16+32+128)

    my $read2start;
    ($strand eq "-") ? ($read2start = $start) : ($read2start = $stop - length($actual_seq2) + 1);

    my $cigar2 = length($actual_seq2) . "M";                      # Assume no indel during mapping (only matches and mismatches)
    my $mpos2;                                                    # Left-most position of mate
    ($strand eq "-") ?  ($mpos2 = $stop - length($actual_seq1)+1) : ($mpos2 = $start);

    my $isize2;
    $isize2 = -1 * $isize1;

    if ($strand eq '+'){
      $actual_seq2 = revcomp($actual_seq2);                       # Mate sequence represented on the forward genomic strand
      $ref_seq2 = revcomp($ref_seq2);                             # Required for comparison with actual sequence
      $qual2 = reverse $qual2;                                    # If the sequence gets reverse complemented we reverse the quality string as well
    }

    $hemming_dist = hemming_dist($actual_seq2, $ref_seq2);
    my $NM_tag2 = "NM:i:$hemming_dist";                           # Optional tag: edit distance based on nucleotide differences
    my $MD_tag2 = make_mismatch_string($actual_seq2, $ref_seq2);  # Optional tag: String to provide mismatch and indel information. Expect only mismatches

    my $XM_tag2;                                                  # Optional tag: String to provide Methylation calls
    if ($strand eq '+'){
      $XM_tag2 = 'XM:Z:'.reverse $methcall2;                      # Needs to be reversed if the sequence was reverse complemented
    }
    elsif ($strand eq '-'){
      $XM_tag2 = "XM:Z:$methcall2";
    }
    else{
      die "Strand was neither + nor - but: $strand\n";
    }


    # SAM format: QNAME, FLAG, RNAME, 1-based START, MAPQ, CIGAR, MRNM, MPOS, ISIZE, SEQ, QUAL
    if ($sorting){
      $seqs1{$count} = {
			id => $id1,
			flag => $flag1,
			chr => $chr,
			start => $read1start,
			mapq => $mapq,
			cigar => $cigar1,
			mrnm => $mrnm,
			mpos => $mpos1,
			isize => $isize1,
			actual_seq => $actual_seq1,
			qual => $qual1,
			NM_tag => $NM_tag1,
			MD_tag => $MD_tag1,
			XM_tag => $XM_tag1,
		       }
	;

      $seqs2{$count} = {
			id => $id2,
			flag => $flag2,
			chr => $chr,
			start => $read2start,
			mapq => $mapq,
			cigar => $cigar2,
			mrnm => $mrnm,
			mpos => $mpos2,
			isize => $isize2,
			actual_seq => $actual_seq2,
			qual => $qual2,
			NM_tag => $NM_tag2,
			MD_tag => $MD_tag2,
			XM_tag => $XM_tag2,
		       }
	;
    }
    else{ ### default
      print OUT join("\t", ($id1, $flag1, $chr, $read1start, $mapq, $cigar1, $mrnm, $mpos1, $isize1, $actual_seq1, $qual1, $NM_tag1, $MD_tag1, $XM_tag1)), "\n";
      print OUT join("\t", ($id2, $flag2, $chr, $read2start, $mapq, $cigar2, $mrnm, $mpos2, $isize2, $actual_seq2, $qual2, $NM_tag2, $MD_tag2, $XM_tag2)), "\n";
    }
  }

  ## sorting output by chromosome and start position
  if ($sorting){
    foreach my $seq (sort {$seqs1{$a}->{chr} cmp $seqs1{$b}->{chr} || $seqs1{$a}->{start}<=>$seqs1{$b}->{start}} keys %seqs1){

      ##seq1
      print OUT join("\t", ($seqs1{$seq}->{id}, $seqs1{$seq}->{flag}, $seqs1{$seq}->{chr}, $seqs1{$seq}->{start}, $seqs1{$seq}->{mapq}, $seqs1{$seq}->{cigar},$seqs1{$seq}->{mrnm}, $seqs1{$seq}->{mpos}, $seqs1{$seq}->{isize}, $seqs1{$seq}->{actual_seq}, $seqs1{$seq}->{qual}, $seqs1{$seq}->{NM_tag}, $seqs1{$seq}->{MD_tag},$seqs1{$seq}->{XM_tag})), "\n";

      ##seq2
      print OUT join("\t", ($seqs2{$seq}->{id}, $seqs2{$seq}->{flag}, $seqs2{$seq}->{chr}, $seqs2{$seq}->{start}, $seqs2{$seq}->{mapq}, $seqs2{$seq}->{cigar},$seqs2{$seq}->{mrnm}, $seqs2{$seq}->{mpos}, $seqs2{$seq}->{isize}, $seqs2{$seq}->{actual_seq}, $seqs2{$seq}->{qual}, $seqs2{$seq}->{NM_tag}, $seqs2{$seq}->{MD_tag},$seqs1{$seq}->{XM_tag})), "\n";
    }
  }
}

sub revcomp{
  my $seq = shift or croak "Missing seq to reverse complement\n";
  $seq = reverse $seq;
  $seq =~ tr/ACTGactg/TGACtgac/;
  return $seq;
}

sub hemming_dist{
  my $string1 = shift or croak "Missing string 1\n";
  my $string2 = shift or croak "Missing string 2\n";
  return my $hd = length($string1) - (($string1 ^ $string2) =~ tr/[\0]/[\0]/);
}

sub make_mismatch_string{
  my $actual_seq = shift or croak "Missing actual sequence";
  my $ref_seq = shift or croak "Missing reference sequence";
  my $MD_tag = "MD:Z:";
  my $tmp = ($actual_seq ^ $ref_seq);                    # Bitwise comparison
  my $prev_mm_pos = 0;
  while($tmp =~ /[^\0]/g){                               # Where bitwise comparison showed a difference
    my $nuc_match = pos($tmp) - $prev_mm_pos - 1;        # Generate number of nucleotide that matches since last mismatch
    my $nuc_mm = substr($actual_seq, pos($tmp) - 1, 1) if pos($tmp) <= length($actual_seq);  # Obtain nucleotide that was different from reference
    $MD_tag .= "$nuc_match" if $nuc_match > 0;           # Ignore if mismatches are adjacent to each other
    $MD_tag .= "$nuc_mm" if defined $nuc_mm;             # Ignore if there is no mismatch (prevents uninitialized string concatenation)
    $prev_mm_pos = pos($tmp);                            # Position of last mismatch
  }
  my $end_matches = length($actual_seq) - $prev_mm_pos;  # Provides number of matches from last mismatch till end of sequence
  $MD_tag .= "$end_matches" if $end_matches > 0;         # Ignore if mismatch is at the end of sequence
  return $MD_tag;
}


sub usage{
  print <<EOF

Usage:
   bismark_to_SAM.pl (-p) -c [chrom sizes file] -i [bismark mapping output] -o [SAM output]

  -p/paired_end                        -  paired-end bismark mapping results (default is single-end)
  -s/sorting                           -  sorts the SAM output by chromosome and start position (optional)
  -c/chrom_file [chrom sizes file]     -  file containing length of chromosomes/sequences used for Bowtie mapping
  -i/infile [bismark mapping output]   -  file containing Bismark mapping output
  -o/outfile [SAM output]              -  name for output file in SAM format (default: [input].sam)
  --version                            -  display version information and quit

EOF
    ;
  exit 1;
}
