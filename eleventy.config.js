const fs = require("node:fs");
const path = require("node:path");
const markdownIt = require("markdown-it");

const repositoryUrl = "https://github.com/djpardis/trailmix";
const readmePath = path.join(__dirname, "README.md");
const architecturePath = path.join(__dirname, "ARCHITECTURE.md");
const transformerPath = path.join(__dirname, "TRANSFORMER.md");

function decorateProjectNames(html) {
  const decorated = html
    .split(/(<[^>]+>)/g)
    .map((part) => {
      if (part.startsWith("<")) {
        return part;
      }

      return part
        .replace(/\bbeat salad's\b/g, '<strong class="part-name">beat salad\'s</strong>')
        .replace(/\bkey lime's\b/g, '<strong class="part-name">key lime\'s</strong>')
        .replace(/\bsampler platter's\b/g, '<strong class="part-name">sampler platter\'s</strong>')
        .replace(/\btrail mix\b/g, '<strong class="project-name">trail mix</strong>')
        .replace(/\bbeat salad\b/g, '<strong class="part-name">beat salad</strong>')
        .replace(/\bbeat-salad\b/g, '<strong class="part-name">beat-salad</strong>')
        .replace(/\bkey lime\b/g, '<strong class="part-name">key lime</strong>')
        .replace(/\bkey-lime\b/g, '<strong class="part-name">key-lime</strong>')
        .replace(/\bsampler platter\b/g, '<strong class="part-name">sampler platter</strong>')
        .replace(/\bsampler-platter\b/g, '<strong class="part-name">sampler-platter</strong>');
    })
    .join("");

  return decorated
    .replace(
      /<strong><strong class="(project-name|part-name)">([^<]+)<\/strong><\/strong>/g,
      '<strong class="$1">$2</strong>',
    )
    .replace(
      /<(h3|span) class="part-name"><strong class="part-name">([^<]+)<\/strong><\/\1>/g,
      '<$1 class="part-name">$2</$1>',
    )
    .replace(
      /<strong class="part-name"><strong class="part-name">([^<]+)<\/strong>'s<\/strong>/g,
      '<strong class="part-name">$1\'s</strong>',
    );
}

function rewriteRepositoryLinks(html) {
  const rewritten = html
    .replace(
      /href="(?!https?:|mailto:|#)([^"]+)"/g,
      (_, target) =>
        target === "ARCHITECTURE.md"
          ? 'href="/architecture/"'
          : target === "TRANSFORMER.md"
            ? 'href="/transformer/"'
            : `href="${repositoryUrl}/blob/main/${target}"`,
    )
    .replace(/src="docs\/([^"/]+)"/g, (_, name) => `src="/assets/${name}?v=2"`)
    .replace(
      /src="(?!https?:|data:|\/)([^"]+)"/g,
      'src="https://raw.githubusercontent.com/djpardis/trailmix/main/$1"',
    );

  return decorateProjectNames(rewritten);
}

function removeBadgeParagraphs(tokens) {
  const filtered = [];

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    const inline = tokens[index + 1];
    const isBadgeParagraph =
      token.type === "paragraph_open" &&
      inline?.type === "inline" &&
      inline.children?.some((child) => child.type === "image");

    if (isBadgeParagraph) {
      index += 2;
      continue;
    }

    filtered.push(token);
  }

  return filtered;
}

function isDecorativeReadmeBlock(block) {
  return (
    block.startsWith("#") ||
    block.startsWith("[![") ||
    block.startsWith("<p align=\"center\">") ||
    block.startsWith("```")
  );
}

module.exports = function (eleventyConfig) {
  const markdown = markdownIt({
    html: true,
    linkify: true,
    typographer: false,
  });
  const defaultLinkOpen =
    markdown.renderer.rules.link_open ||
    ((tokens, index, options, env, self) => self.renderToken(tokens, index, options));

  markdown.renderer.rules.link_open = (tokens, index, options, env, self) => {
    const href = tokens[index].attrGet("href") ?? "";

    if (/^https?:\/\//.test(href)) {
      tokens[index].attrSet("target", "_blank");
      tokens[index].attrSet("rel", "noopener noreferrer");
    }

    return defaultLinkOpen(tokens, index, options, env, self);
  };

  eleventyConfig.addPassthroughCopy({ "docs/styles.css": "styles.css" });
  eleventyConfig.addPassthroughCopy({
    "brand/illustrations/trail-mix.png": "assets/trail-mix.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/trail-mix-social-card.png": "assets/trail-mix-social-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/beat-salad-card.png": "assets/beat-salad-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/key-lime-card.png": "assets/key-lime-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/sampler-platter-card.png": "assets/sampler-platter-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/key-lime-accuracy-footprint.svg": "assets/key-lime-accuracy-footprint.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/key-lime-reranker-flow.svg": "assets/key-lime-reranker-flow.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/crate-architecture.svg": "assets/crate-architecture.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/beat-salad-pipeline.svg": "assets/beat-salad-pipeline.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/key-lime-pipeline.svg": "assets/key-lime-pipeline.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/chroma-folding.svg": "assets/chroma-folding.svg",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/waveform-columns.svg": "assets/waveform-columns.svg",
  });
  eleventyConfig.addPassthroughCopy("docs/favicon-32x32.png");
  eleventyConfig.addPassthroughCopy("docs/apple-touch-icon.png");
  eleventyConfig.addPassthroughCopy("docs/icon-192.png");
  eleventyConfig.addPassthroughCopy("docs/icon-512.png");
  eleventyConfig.addPassthroughCopy("docs/site.webmanifest");
  eleventyConfig.addPassthroughCopy("docs/robots.txt");
  eleventyConfig.addPassthroughCopy("docs/sitemap.xml");
  eleventyConfig.addPassthroughCopy("docs/CNAME");
  eleventyConfig.addPassthroughCopy("docs/.nojekyll");
  eleventyConfig.addWatchTarget("README.md");
  eleventyConfig.addWatchTarget("ARCHITECTURE.md");
  eleventyConfig.addWatchTarget("TRANSFORMER.md");

  eleventyConfig.addGlobalData("assetVersion", () => Date.now());
  eleventyConfig.addGlobalData("year", () => new Date().getFullYear());

  eleventyConfig.addGlobalData("readme", () => {
    const source = fs.readFileSync(readmePath, "utf8");
    const tokens = markdown.parse(source, {});
    const title = source.match(/^#\s+(.+)$/m)?.[1] ?? "trail mix";
    const description =
      source
        .split(/\n\s*\n/)
        .map((block) => block.trim())
        .find(
          (block) =>
            block &&
            !isDecorativeReadmeBlock(block),
        ) ?? "";

    const firstHeadingClose = tokens.findIndex(
      (token) => token.type === "heading_close" && token.tag === "h1",
    );
    const firstSection = tokens.findIndex(
      (token) => token.type === "heading_open" && token.tag === "h2",
    );
    const introTokens = removeBadgeParagraphs(
      tokens.slice(firstHeadingClose + 1, firstSection),
    );
    const sections = [];

    for (let index = firstSection; index < tokens.length; index += 1) {
      if (tokens[index].type !== "heading_open" || tokens[index].tag !== "h2") {
        continue;
      }

      const sectionTitle = tokens[index + 1].content;
      let nextSection = index + 3;
      while (
        nextSection < tokens.length &&
        (tokens[nextSection].type !== "heading_open" ||
          tokens[nextSection].tag !== "h2")
      ) {
        nextSection += 1;
      }

      const sectionHtml = rewriteRepositoryLinks(
        markdown.renderer.render(
          tokens.slice(index + 3, nextSection),
          markdown.options,
          {},
        ),
      );

      sections.push({
        title: sectionTitle,
        slug: sectionTitle.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
        html: sectionHtml,
        firstSentenceHtml: `${sectionHtml.split(". ")[0]}.</p>`,
      });
      index = nextSection - 1;
    }

    return {
      title,
      description: description.replace(/\n/g, " "),
      introHtml: rewriteRepositoryLinks(
        markdown.renderer.render(introTokens, markdown.options, {}),
      ),
      sections,
    };
  });

  eleventyConfig.addGlobalData("architecture", () => {
    const source = fs.readFileSync(architecturePath, "utf8");
    const tokens = markdown.parse(source, {});
    const title = source.match(/^#\s+(.+)$/m)?.[1] ?? "Architecture";
    const firstHeadingClose = tokens.findIndex(
      (token) => token.type === "heading_close" && token.tag === "h1",
    );
    const firstSection = tokens.findIndex(
      (token) => token.type === "heading_open" && token.tag === "h2",
    );
    const sections = [];

    for (let index = firstSection; index < tokens.length; index += 1) {
      if (tokens[index].type !== "heading_open" || tokens[index].tag !== "h2") {
        continue;
      }

      const sectionTitle = tokens[index + 1].content;
      let nextSection = index + 3;
      while (
        nextSection < tokens.length &&
        (tokens[nextSection].type !== "heading_open" ||
          tokens[nextSection].tag !== "h2")
      ) {
        nextSection += 1;
      }

      sections.push({
        title: sectionTitle,
        slug: sectionTitle.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
        html: rewriteRepositoryLinks(
          markdown.renderer.render(
            tokens.slice(index + 3, nextSection),
            markdown.options,
            {},
          ),
        ),
      });
      index = nextSection - 1;
    }

    return {
      title,
      description: "trail mix architecture: analyzer algorithms, execution flow, the result contract, and references.",
      introHtml:
        firstHeadingClose >= 0 && firstSection > firstHeadingClose
          ? rewriteRepositoryLinks(
              markdown.renderer.render(
                tokens.slice(firstHeadingClose + 1, firstSection),
                markdown.options,
                {},
              ),
            )
          : "<p>trail mix architecture: analyzer algorithms, execution flow, the result contract, and references.</p>",
      sections,
    };
  });

  eleventyConfig.addGlobalData("transformer", () => {
    const source = fs.readFileSync(transformerPath, "utf8");
    const tokens = markdown.parse(source, {});
    const title = source.match(/^#\s+(.+)$/m)?.[1] ?? "Key lime transformer";
    const firstHeadingClose = tokens.findIndex(
      (token) => token.type === "heading_close" && token.tag === "h1",
    );
    const firstSection = tokens.findIndex(
      (token) => token.type === "heading_open" && token.tag === "h2",
    );
    const sections = [];

    for (let index = firstSection; index < tokens.length; index += 1) {
      if (tokens[index].type !== "heading_open" || tokens[index].tag !== "h2") {
        continue;
      }

      const sectionTitle = tokens[index + 1].content;
      let nextSection = index + 3;
      while (
        nextSection < tokens.length &&
        (tokens[nextSection].type !== "heading_open" ||
          tokens[nextSection].tag !== "h2")
      ) {
        nextSection += 1;
      }

      sections.push({
        title: sectionTitle,
        slug: sectionTitle.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
        html: rewriteRepositoryLinks(
          markdown.renderer.render(
            tokens.slice(index + 3, nextSection),
            markdown.options,
            {},
          ),
        ),
      });
      index = nextSection - 1;
    }

    return {
      title,
      description: "Compact transformer reranker for trail mix key estimation.",
      introHtml:
        firstHeadingClose >= 0 && firstSection > firstHeadingClose
          ? rewriteRepositoryLinks(
              markdown.renderer.render(
                tokens.slice(firstHeadingClose + 1, firstSection),
                markdown.options,
                {},
              ),
            )
          : "<p>Compact transformer reranker for trail mix key estimation.</p>",
      sections,
    };
  });

  return {
    dir: {
      input: "docs",
      output: "_site",
    },
    templateFormats: ["njk"],
    htmlTemplateEngine: "njk",
  };
};
