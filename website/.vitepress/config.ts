import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Ngalir',
  description: 'Flow automation engine',
  cleanUrls: true,

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Home', link: '/' },
      { text: 'Install', link: '/guide/install' },
      { text: 'Nodes', link: '/nodes/' },
      { text: 'CLI', link: '/guide/cli' },
      { text: 'Docs', link: '/docs/architecture' },
      { text: 'Roadmap', link: '/docs/roadmap' },
    ],

    sidebar: {
      '/guide/': [
        { text: 'Install', link: '/guide/install' },
        { text: 'CLI Reference', link: '/guide/cli' },
        { text: 'Writing Flows', link: '/guide/writing-flows' },
      ],
      '/nodes/': [
        { text: 'Overview', link: '/nodes/' },
      ],
      '/docs/': [
        { text: 'Architecture', link: '/docs/architecture' },
        { text: 'Node Contract', link: '/docs/node-contract' },
        { text: 'Flow Spec', link: '/docs/flow-spec' },
        { text: 'Roadmap', link: '/docs/roadmap' },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/sonyarianto/ngalir' },
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright Sony Arianto Kurniawan',
    },
  },
})
