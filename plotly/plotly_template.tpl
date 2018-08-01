<!DOCTYPE html>
<html lang="en">

	<head>

		<meta http-equiv="content-type" content="text/html; charset=UTF-8">
		<title>Bismark Processing Report - {{filename}}</title>

		<style>
                  body {
                          font-family: Arial, sans-serif;
                          font-size:14px;
                          padding:0 20px 20px;
                  }
                  .container {
                          margin:0 auto;
                          max-width:1200px;
                  }
                  .header h1,
                  .header img {
                          float:left;
                  }
                  .header h1 {
                          margin: 20px 0 10px;
                  }
                  .header img {
                          padding: 0 20px 20px 0;
                  }
                  .subtitle {
                          margin-top:120px;
                          float:right;
                          text-align:right;
                  }
                  .header_subtitle h3,
                  .header_subtitle p {
                          margin:0;
                  }
                  h1 {
                          font-size: 3.2em;
                  }
                  h2 {
                          font-size:2.2em;
                  }
                  h3 {
                          font-size:1.4em;
                  }
                  h2, h3, hr {
                          clear:both;
                  }
                  hr {
                        border-top:1px solid #CCC;
                        border-bottom:1px solid #F3F3F3;
                        border-left:0;
                        border-right:0;
                        height:0;
                  }
				  .bargraph {
						width:600px;
				  }
				  .plotly_table {
                         float:right;
                         width:600px;
                         max-width:100%;
                                    }
                  .data {
                          float:left;
                          width:500px;
                          max-width:100%;
                          margin-right:30px;
                          border:1px solid #CCC;
                          border-collapse:separate;
                          border-spacing: 0;
                          border-left:0;
                          -webkit-border-radius:4px;
                          -moz-border-radius:4px;
                          border-radius:4px;
                  }
                  .data th, .data td {
                          border-left:1px solid #CCC;
                          border-top:1px solid #CCC;
                          padding: 5px 7px;
                  }
                  .data tr:first-child th,
                  .data tr:first-child td {
                          border-top:0;
                  }
                  .data tr:last-child th,
                  .data tr:last-child td {
                          border-bottom: 2px solid #666;
                  }
                  .plot {
                          width:650px;
                          max-width:100%;
                          float:left;
                          margin-bottom:30px;
                  }

                  .fullWidth_plot {
                          height: 600px;
                  }

                  .data th {
                          text-align:left;
                  }
                  .data td {
                          text-align:right;
                  }
                footer {
                    color:#999;
                }
                footer a {
					color:#999;
                }
                .error-msg {
                    color: #a94442;
					background-color: #f2dede;
					border: 1px solid #ebccd1;
					padding: 15px;
					margin-bottom: 20px;
					border-radius: 4px;
                }
				.error-msg h3 { margin: 0; }
				.error-msg pre { margin: 0; }
          </style>

	<!-- Plotly.js -->
	{{plotly_goes_here}}
  
 	This will need to be replaced
  
 	{{plotly_goes_here}}
  
	
	</head>


	<body>
	<div class="container">
		<div class="header">
		  {{bismark_logo_goes_here}}
		  This will be replaced with an epic picture of Bismark
		  {{bismark_logo_goes_here}}

			<h1>Bismark Processing Report</h1>
	
			<div class="subtitle">
				<h3>{{filename}}</h3>
				<p>Data processed at {{time}} on {{date}}</p>
			</div>
			
		</div>
	
		<hr id="header_hr">
	
	

	<h2>Alignment Stats</h2>
	<table>
		<tbody>
			<tr>
				<td>
					<table class="data">
						<tbody>
							<tr>
								<th>{{sequences_analysed_in_total}}</th>
								<td>{{total_sequences_alignments}}</td>
							</tr>
						</tbody>
						<tbody>
							<tr>
								<th>{{unique_seqs_text}}</th>
								<td>{{unique_seqs}}</td>
							</tr>
							<tr>
								<th>{{no_alignments_text}}</th>
								<td>{{no_alignments}}</td>
							</tr>
							<tr>
								<th>{{multiple_alignments_text}}</th>
								<td>{{multiple_alignments}}</td>
							</tr>
							<tr>
								<th>Genomic sequence context not extractable (edges of chromosomes)</th>
								<td>{{no_genomic}}</td>
							</tr>
						</tbody>
					</table>
				</td>
				<td>
					<div id="Bismark_Processing_Report"><!-- Plotly chart will be drawn inside this DIV </div> --></div>
				</td>
			</tr>
		</tbody>
	</table>

	<hr>
	
	<h2>Cytosine Methylation</h2>
	
	<table>
		<tbody>
			<tr>
				<td>
					<table class="data">
						<tbody>
							<tr>
								<th>Total C's analysed</th>
								<td>{{total_C_count}}</td>
							</tr>
						</tbody>
						<tbody>
							<tr>
								<th>Methylated C's in CpG context</th>
								<td>{{meth_CpG}}</td>
							</tr>
							<tr>
								<th>Methylated C's in CHG context</th>
								<td>{{meth_CHG}}</td>
							</tr>
							<tr>
								<th>Methylated C's in CHH context</th>
								<td>{{meth_CHH}}</td>
							</tr>
							{{meth_unknown}}
						</tbody>
						<tbody>
							<tr>
								<th>Unmethylated C's in CpG context</th>
								<td>{{unmeth_CpG}}</td>
							</tr>
							<tr>
								<th>Unmethylated C's in CHG context</th>
								<td>{{unmeth_CHG}}</td>
							</tr>
							<tr>
								<th>Unmethylated C's in CHH context</th>
								<td>{{unmeth_CHH}}</td>
							</tr>
							{{unmeth_unknown}}
						</tbody>
						<tbody>
							<tr>
								<th>Percentage methylation (CpG context)</th>
								<td>{{perc_CpG}}%</td>
							</tr>
							<tr>
								<th>Percentage methylation (CHG context)</th>
								<td>{{perc_CHG}}%</td>
							</tr>
							<tr>
								<th>Percentage methylation (CHH context)</th>
								<td>{{perc_CHH}}%</td>
							</tr>
							{{perc_unknown}}
						</tbody>
					</table>				
				</td>
				<td>
					<div id="Cytosine_Methylation" class="bargraph"><!-- Plotly chart will be drawn inside this DIV --></div>
				</td>
			</tr>
		</tbody>
	</table>
			
	<hr>

	<h2>Alignment to Individual Bisulfite Strands</h2>
	<table>
		<tbody>
			<tr>
				<td>
					<table class="data">
						<tbody>
							<tr>
								<th>OT</th>
								<td>{{number_OT}}</td>
								<td>original top strand</td>
							</tr>
							<tr>
								<th>CTOT</th>
								<td>{{number_CTOT}}</td>
								<td>complementary to original top strand</td>
							</tr>
							<tr>
								<th>CTOB</th>
								<td>{{number_CTOB}}</td>
								<td>complementary to original bottom strand</td>
							</tr>
							<tr>
								<th>OB</th>
								<td>{{number_OB}}</td>
								<td>original bottom strand</td>
							</tr>
						</tbody>
					</table>			
				</td>
				<td>
					<div id="Strand_Plot" class="bargraph"> <!--Plotly chart will be drawn inside this DIV --> </div>
				</td>
			</tr>
		</tbody>
	</table>
	
	<hr>
	
	<!-- This section is optional -->
	{{deduplication_section}}
	<h2>Deduplication</h2>
	<table>
		<tbody>
			<tr>
				<td>
					
					<table class="data">
						<tbody>
						<tr>
							<th>Alignments analysed</th>
							<td>{{seqs_total_duplicates}}</td>
						</tr>
						<tr>
							<th>Unique alignments</th>
							<td>{{unique_alignments_duplicates}}</td>
						</tr>
						<tr>
							<th>Duplicates removed</th>
							<td>{{duplicate_alignments_duplicates}}</td>
						</tr>
						</tbody>
						<tbody>
							<tr>
								<td colspan="2" style="text-align:left;">Duplicated alignments were found at <strong>{{different_positions_duplicates}}</strong> different positions</td>
							</tr>
						</tbody>
					</table>			
				</td>
				<td>
				 	<div id="Duplication_Plot"> <!--Plotly chart will be drawn inside this DIV --> </div>	
				</td>
			</tr>
		</tbody>
	</table>
		
	<hr>
	{{deduplication_section}}
	
	<!-- This section is optional -->
	{{cytosine_methylation_post_deduplication_section}}
	<h2>Cytosine Methylation after Extraction</h2>
	<table>
		<tbody>
			<tr>
				<td>
					<table class="data">
						<tbody>
							<tr>
								<th>Total C's analysed</th>
								<td>{{total_C_count_splitting}}</td>
							</tr>
						</tbody>
						<tbody>
							<tr>
								<th>Methylated C's in CpG context</th>
								<td>{{meth_CpG_splitting}}</td>
							</tr>
							<tr>
								<th>Methylated C's in CHG context</th>
								<td>{{meth_CHG_splitting}}</td>
							</tr>
							<tr>
								<th>Methylated C's in CHH context</th>
								<td>{{meth_CHH_splitting}}</td>
							</tr>
							{{meth_unknown_splitting}}
						</tbody>
						<tbody>
							<tr>
								<th>Unmethylated C's in CpG context</th>
								<td>{{unmeth_CpG_splitting}}</td>
							</tr>
							<tr>
								<th>Unmethylated C's in CHG context</th>
								<td>{{unmeth_CHG_splitting}}</td>
							</tr>
							<tr>
								<th>Unmethylated C's in CHH context</th>
								<td>{{unmeth_CHH_splitting}}</td>
							</tr>
							{{unmeth_unknown_splitting}}	
						</tbody>
						<tbody>
							<tr>
								<th>Percentage methylation (CpG context)</th>
								<td>{{perc_CpG_splitting}}%</td>
							</tr>
							<tr>
								<th>Percentage methylation (CHG context)</th>
								<td>{{perc_CHG_splitting}}%</td>
							</tr>
							<tr>
								<th>Percentage methylation (CHH context)</th>
								<td>{{perc_CHH_splitting}}%</td>
							</tr>
							{{perc_unknown_splitting}}
						</tbody>
					</table>				
				</td>
				<td>
					<div id="Cytosine_Methylation_postDuplication" class="bargraph"> <!--Plotly chart will be drawn inside this DIV --> </div>	
				</td>
			</tr>
		</tbody>
	</table>	

	<hr>
	{{cytosine_methylation_post_deduplication_section}}
		
	<!-- This section is optional -->
	{{nucleotide_coverage_section}}
	<h2>Nucleotide Coverage</h2>
	
	<table>
		<tbody>
			<tr>
				<td>
					<table class="data" id="nucleotide_coverage_table">
						<thead>
							<tr><th>Nucleotide Class</th> <th>Counts Sample</th> <th>Counts Genome</th><th>% in Sample</th> <th>% in Genome</th> <th>fold coverage</th></thead>
						<tbody>
							<tr><th>A</th>	<td>{{nuc_A_counts_obs}}</td> <td>{{nuc_A_counts_exp}}</td> <td>{{nuc_A_p_obs}}</td>  <td>{{nuc_A_p_exp}}</td>	<td>{{nuc_A_coverage}}</td></tr>
							<tr><th>T</th>	<td>{{nuc_T_counts_obs}}</td> <td>{{nuc_T_counts_exp}}</td> <td>{{nuc_T_p_obs}}</td>  <td>{{nuc_T_p_exp}}</td>  <td>{{nuc_T_coverage}}</td></tr>
							<tr><th>C</th>	<td>{{nuc_C_counts_obs}}</td> <td>{{nuc_C_counts_exp}}</td> <td>{{nuc_C_p_obs}}</td>  <td>{{nuc_C_p_exp}}</td>  <td>{{nuc_C_coverage}}</td></tr>
							<tr><th>G</th>	<td>{{nuc_G_counts_obs}}</td> <td>{{nuc_G_counts_exp}}</td> <td>{{nuc_G_p_obs}}</td>  <td>{{nuc_G_p_exp}}</td>	<td>{{nuc_G_coverage}}</td></tr>
						</tbody>
						<tbody>
							<tr><th>AC</th>	<td>{{nuc_AC_counts_obs}}</td> <td>{{nuc_AC_counts_exp}}</td> <td>{{nuc_AC_p_obs}}</td> <td>{{nuc_AC_p_exp}}</td> <td>{{nuc_AC_coverage}}</tr>
							<tr><th>CA</th>	<td>{{nuc_CA_counts_obs}}</td> <td>{{nuc_CA_counts_exp}}</td> <td>{{nuc_CA_p_obs}}</td> <td>{{nuc_CA_p_exp}}</td> <td>{{nuc_CA_coverage}}</tr>
							<tr><th>TC</th>	<td>{{nuc_TC_counts_obs}}</td> <td>{{nuc_TC_counts_exp}}</td> <td>{{nuc_TC_p_obs}}</td> <td>{{nuc_TC_p_exp}}</td> <td>{{nuc_TC_coverage}}</tr>
							<tr><th>CT</th>	<td>{{nuc_CT_counts_obs}}</td> <td>{{nuc_CT_counts_exp}}</td> <td>{{nuc_CT_p_obs}}</td> <td>{{nuc_CT_p_exp}}</td> <td>{{nuc_CT_coverage}}</tr>
							<tr><th>CC</th>	<td>{{nuc_CC_counts_obs}}</td> <td>{{nuc_CC_counts_exp}}</td> <td>{{nuc_CC_p_obs}}</td> <td>{{nuc_CC_p_exp}}</td> <td>{{nuc_CC_coverage}}</tr>
							<tr><th>CG</th>	<td>{{nuc_CG_counts_obs}}</td> <td>{{nuc_CG_counts_exp}}</td> <td>{{nuc_CG_p_obs}}</td> <td>{{nuc_CG_p_exp}}</td> <td>{{nuc_CG_coverage}}</tr>
							<tr><th>GC</th>	<td>{{nuc_GC_counts_obs}}</td> <td>{{nuc_GC_counts_exp}}</td> <td>{{nuc_GC_p_obs}}</td> <td>{{nuc_GC_p_exp}}</td> <td>{{nuc_GC_coverage}}</tr>
							<tr><th>GG</th>	<td>{{nuc_GG_counts_obs}}</td> <td>{{nuc_GG_counts_exp}}</td> <td>{{nuc_GG_p_obs}}</td> <td>{{nuc_GG_p_exp}}</td> <td>{{nuc_GG_coverage}}</tr>
							<tr><th>AG</th>	<td>{{nuc_AG_counts_obs}}</td> <td>{{nuc_AG_counts_exp}}</td> <td>{{nuc_AG_p_obs}}</td> <td>{{nuc_AG_p_exp}}</td> <td>{{nuc_AG_coverage}}</tr>
							<tr><th>GA</th>	<td>{{nuc_GA_counts_obs}}</td> <td>{{nuc_GA_counts_exp}}</td> <td>{{nuc_GA_p_obs}}</td> <td>{{nuc_GA_p_exp}}</td> <td>{{nuc_GA_coverage}}</tr>
							<tr><th>TG</th>	<td>{{nuc_TG_counts_obs}}</td> <td>{{nuc_TG_counts_exp}}</td> <td>{{nuc_TG_p_obs}}</td> <td>{{nuc_TG_p_exp}}</td> <td>{{nuc_TG_coverage}}</tr>
							<tr><th>GT</th>	<td>{{nuc_GT_counts_obs}}</td> <td>{{nuc_GT_counts_exp}}</td> <td>{{nuc_GT_p_obs}}</td> <td>{{nuc_GT_p_exp}}</td> <td>{{nuc_GT_coverage}}</tr>
							<tr><th>TT</th>	<td>{{nuc_TT_counts_obs}}</td> <td>{{nuc_TT_counts_exp}}</td> <td>{{nuc_TT_p_obs}}</td> <td>{{nuc_TT_p_exp}}</td> <td>{{nuc_TT_coverage}}</tr>	
							<tr><th>TA</th>	<td>{{nuc_TA_counts_obs}}</td> <td>{{nuc_TA_counts_exp}}</td> <td>{{nuc_TA_p_obs}}</td> <td>{{nuc_TA_p_exp}}</td> <td>{{nuc_TA_coverage}}</tr>
							<tr><th>TA</th>	<td>{{nuc_TA_counts_obs}}</td> <td>{{nuc_TA_counts_exp}}</td> <td>{{nuc_TA_p_obs}}</td> <td>{{nuc_TA_p_exp}}</td> <td>{{nuc_TA_coverage}}</tr>
							<tr><th>AT</th>	<td>{{nuc_AT_counts_obs}}</td> <td>{{nuc_AT_counts_exp}}</td> <td>{{nuc_AT_p_obs}}</td> <td>{{nuc_AT_p_exp}}</td> <td>{{nuc_AT_coverage}}</tr>
							<tr><th>AA</th>	<td>{{nuc_AA_counts_obs}}</td> <td>{{nuc_AA_counts_exp}}</td> <td>{{nuc_AA_p_obs}}</td> <td>{{nuc_AA_p_exp}}</td> <td>{{nuc_AA_coverage}}</tr>
						</tbody>
					</table>			
				</td>
				<td>
					<div id="nucleo_plot" class="bargraph"> <!--Plotly chart will be drawn inside this DIV --> </div>	
				</td>
			</tr>
		</tbody>
	</table>
	
	<hr>
	{{nucleotide_coverage_section}}
	
	
	<!-- This section is optional -->
	{{mbias_r1_section}}
	<h2>M-Bias Plot - Read 1</h2>
	
	
	{{mbias_r1_section}}
	<hr>
	
	<!-- This section is optional -->
	{{mbias_r2_section}}	
	<h2>M-Bias Plot - Read 2</h2>	
	
	
	{{mbias_r2_section}}

	<hr>
	
	<!-- ######################### PLOT.LY plotting code below ################################################################# -->

	<!-- Alignment Stats Plot -->	
	<script>
		<!-- JAVASCRIPT CODE GOES HERE -->

		var data = [{
			<!-- values: [19, 26, 55, 2], hardcoded for testing -->
			values: [{{alignment_stats_plotly}}],
			labels: ['Unique Alignments', 'No Alignment', 'Multiple Alignments', 'No Genomic Sequence'],
			type: 'pie',
			name: 'Alignment Statistics',
			hoverinfo: 'label+value+percent+name',
		}];

		var layout = {
			height: 600,
			width: 700,
		};

		Plotly.newPlot('Bismark_Processing_Report', data, layout, {displaylogo: false}, {modeBarButtonsToRemove: ['toImage',
					'sendDataToCloud',
					'resetScale2d',
					'hoverClosestCartesian',
                    'hoverCompareCartesian',
                    'toggleSpikelines']}
					);
		
		
	</script>

	<!-- Duplication Plot (Donut Plot)-->	
	<script>
		
	var data = [{
		<!-- values: [86, 14], hardcoded for testing purposes -->
		values: [{{duplication_stats_plotly}}],
		labels: ['Unique Alignments', 'Duplicate Alignments' ],
		
		name: 'Degree of Duplication',
		hoverinfo: 'label+value+percent+name',
		hole: .3,	
		type: 'pie',
	 	}];

	
	var layout = {
	  height: 400,	  
	  width: 600
	};

	Plotly.newPlot('Duplication_Plot', data, layout, {displaylogo: false}, {modeBarButtonsToRemove: ['toImage',
					'sendDataToCloud',
					'resetScale2d',
					'hoverClosestCartesian',
                    'hoverCompareCartesian',
                    'toggleSpikelines']});
	</script>

	<!-- Cytosine Methylation Plot -->
	<script>
		var data = [
		  {
			x: ['CpG context', 'CHG context', 'CHH context'],
			<!-- y: [20, 14, 23], hardcoded for testing purposes -->
			y: [{{cytosine_methylation_plotly}}],
			type: 'bar'
		  }
		];
	
		Plotly.newPlot('Cytosine_Methylation', data, {displaylogo: false}, {modeBarButtonsToRemove: ['toImage',
					'sendDataToCloud',
					'resetScale2d',
					'hoverClosestCartesian',
                    'hoverCompareCartesian',
                    'toggleSpikelines']});
	</script>

	<!-- Cytosine Methylation Plot post duplication-->
	<script>

		var data = [
		  {
			x: ['CpG context', 'CHH context', 'CHG context'],
			<!-- y: [20, 14, 23], hardcoded for testing purposes -->
			y: [{{cytosine_methylation_post_duplication_plotly}}],
			type: 'bar'
		  }
		];

		Plotly.newPlot('Cytosine_Methylation_postDuplication', data, {displaylogo: false}, {modeBarButtonsToRemove: ['toImage',
					'sendDataToCloud',
					'resetScale2d',
					'hoverClosestCartesian',
                    'hoverCompareCartesian',
                    'toggleSpikelines']});
	</script>

	<!-- Strand Alignment Plot-->
	<script>
		var data = [
		  {
			x: ['OT', 'CTOT', 'CTOB', 'OB'],
			<!-- y: [49, 1, 2, 48], hardcoded for testing purposes -->
			y: [{{strand_alignment_plotly}}],
			type: 'bar'
		  }
		];

		Plotly.newPlot('Strand_Plot', data, {displaylogo: false}, {modeBarButtonsToRemove: ['toImage',
					'sendDataToCloud',
					'resetScale2d',
					'hoverClosestCartesian',
                    'hoverCompareCartesian',
                    'toggleSpikelines']});
	</script>

	<!-- Nucleotide Stats Plot-->
	<script>
			var trace1 = {
		  x: [1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2012], 
		  y: [219, 146, 112, 127, 124, 180, 236, 207, 236, 263, 350, 430, 474, 526, 488, 537, 500, 439], 
		  name: 'Rest of world', 
		  		  marker: {color: 'rgb(55, 83, 109)'}, 
		  type: 'bar'
		};

		var trace2 = {
		  x: [1995, 1996, 1997, 1998, 1999, 2000, 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010, 2011, 2012], 
		  y: [16, 13, 10, 11, 28, 37, 43, 55, 56, 88, 105, 156, 270, 299, 340, 403, 549, 499], 
		  name: 'China', 
		 		  marker: {color: 'rgb(26, 118, 255)'}, 
		  type: 'bar'
		};

		var data = 	[trace1, trace2];

		var layout = {
		  title: 'US Export of Plastic Scrap',
		  orientation: 'h',
		  xaxis: {tickfont: {
			  size: 14, 
			  color: 'rgb(107, 107, 107)'
			}}, 
		  yaxis: {
			title: 'USD (millions)',
			titlefont: {
			  size: 16, 
			  color: 'rgb(107, 107, 107)'
			}, 
			tickfont: {
			  size: 14, 
			  color: "rgb(107, 107, 107)"
			}
		  }, 
		  legend: {
			x: 0, 
			y: 1.0, 
			bgcolor: 'rgba(255, 255, 255, 0)',
			bordercolor: 'rgba(255, 255, 255, 0)'
		  }, 
		  barmode: 'group', 
		  bargap: 0.15, 
		  bargroupgap: 0.1
		};

		Plotly.newPlot('nucleo_plot', data, layout);
	</script>

	<footer>
		<a style="float:right;" href="https://www.bioinformatics.babraham.ac.uk/">
		  {{bioinf_logo_goes_here}}
		  This will be replaced with an epic logo of Babraham Bioinf
		  {{bioinf_logo_goes_here}}


		</a>
		<p>Analysis produced by <a href="https://github.com/FelixKrueger/Bismark"><strong>Bismark</strong></a> (version {{bismark_version}}) - a tool to map bisulfite converted sequence reads and determine cytosine methylation states</p>
		<p>Report graphs rendered using <a href="https://plot.ly/">plot.ly</a>, design last changed 15 May 2018</p>
	</footer>

	</div>
</html>
