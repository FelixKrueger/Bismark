// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
  // Project GitHub Pages on FelixKrueger/Bismark: https://felixkrueger.github.io/Bismark/.
  // For a fork preview set site to your user page; for a custom domain set base back to '/'.
  site: 'https://felixkrueger.github.io',
  base: '/Bismark/',
  integrations: [
    starlight({
      title: 'Bismark',
      description:
        'Bismark maps bisulfite-treated sequencing reads to a reference genome and calls cytosine methylation in CpG, CHG and CHH context, in a single pass.',
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/FelixKrueger/Bismark' }],
      editLink: { baseUrl: 'https://github.com/FelixKrueger/Bismark/edit/rust/iron-chancellor/docs/' },
      components: {
        SiteTitle: './src/components/SiteTitle.astro',
        PageTitle: './src/components/PageTitle.astro',
        MarkdownContent: './src/components/MarkdownContent.astro',
        Header: './src/components/Header.astro',
        ThemeSelect: './src/components/ThemeSelect.astro',
      },
      customCss: ['./src/styles/custom.css'],
      // The docs-nav sidebar. Identical on every page (including the custom home).
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Introduction', slug: '' },
            { label: 'Quick reference', slug: 'quick-reference' },
            { label: 'Installation', slug: 'installation' },
          ],
        },
        {
          label: 'Usage',
          items: [
            { label: 'Genome preparation', slug: 'usage/genome-preparation' },
            { label: 'Alignment', slug: 'usage/alignment' },
            { label: 'Deduplication', slug: 'usage/deduplication' },
            { label: 'Methylation extraction', slug: 'usage/methylation-extraction' },
            { label: 'Coverage report', slug: 'usage/coverage-report' },
            { label: 'Processing report', slug: 'usage/processing-report' },
            { label: 'Summary report', slug: 'usage/summary-report' },
            { label: 'Filtering non-conversion', slug: 'usage/filtering-non-conversion' },
            { label: 'Library types', slug: 'usage/library-types' },
            { label: 'Concordance', slug: 'usage/concordance' },
          ],
        },
        {
          label: 'Full list of options',
          items: [
            { label: 'Genome preparation', slug: 'options/genome-preparation' },
            { label: 'Alignment', slug: 'options/alignment' },
            { label: 'Deduplication', slug: 'options/deduplication' },
            { label: 'Methylation extraction', slug: 'options/methylation-extraction' },
          ],
        },
        {
          label: 'Rust rewrite',
          items: [
            { label: 'Benchmarks', slug: 'rust/benchmarks' },
          ],
        },
        {
          label: 'FAQ',
          items: [
            { label: 'Overview', slug: 'faq' },
            { label: 'Single-cell & PBAT', slug: 'faq/single-cell-pbat' },
            { label: 'Low mapping efficiency', slug: 'faq/low-mapping' },
            { label: 'Changing context', slug: 'faq/changing-context' },
            { label: 'Conversion efficiency', slug: 'faq/conversion-efficiency' },
          ],
        },
      ],
    }),
  ],
});
