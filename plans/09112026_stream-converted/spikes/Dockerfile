FROM mambaorg/micromamba:1.5.8-bookworm-slim
USER root
RUN micromamba install -y -n base -c bioconda -c conda-forge \
      bowtie2=2.5.5 hisat2=2.2.2 minimap2=2.31 python=3.12 \
    && micromamba clean --all --yes
ENV PATH=/opt/conda/bin:$PATH
WORKDIR /work
