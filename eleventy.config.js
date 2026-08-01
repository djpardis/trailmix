const fs = require("node:fs");
const path = require("node:path");
const markdownIt = require("markdown-it");

const repositoryUrl = "https://github.com/djpardis/trailmix";
const readmePath = path.join(__dirname, "README.md");
const architecturePath = path.join(__dirname, "ARCHITECTURE.md");

function decorateProjectName(html) {
  const protectedNames = [];
  const protectedHtml = html.replace(
    /<strong(?: class="project-name")?>trail mix<\/strong>/g,
    () => {
      const placeholder = `__PROJECT_NAME_${protectedNames.length}__`;
      protectedNames.push('<strong class="project-name">trail mix</strong>');
      return placeholder;
    },
  );

  return protectedHtml
    .replace(/\btrail mix\b/g, '<strong class="project-name">trail mix</strong>')
    .replace(/__PROJECT_NAME_(\d+)__/g, (_, index) => protectedNames[index]);
}

function rewriteRepositoryLinks(html) {
  const rewritten = html
    .replace(
      /href="(?!https?:|mailto:|#)([^"]+)"/g,
      (_, target) =>
        target === "ARCHITECTURE.md"
          ? 'href="/architecture/"'
          : `href="${repositoryUrl}/blob/main/${target}"`,
    )
    .replace(
      /src="(?!https?:|data:)([^"]+)"/g,
      'src="https://raw.githubusercontent.com/djpardis/trailmix/main/$1"',
    );

  return decorateProjectName(rewritten);
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
    "docs/beat-salad-card.png": "assets/beat-salad-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/key-lime-card.png": "assets/key-lime-card.png",
  });
  eleventyConfig.addPassthroughCopy({
    "docs/sampler-platter-card.png": "assets/sampler-platter-card.png",
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
      description: "trail mix architecture, crate boundaries, result stability, and current limitations.",
      introHtml:
        firstHeadingClose >= 0 && firstSection > firstHeadingClose
          ? rewriteRepositoryLinks(
              markdown.renderer.render(
                tokens.slice(firstHeadingClose + 1, firstSection),
                markdown.options,
                {},
              ),
            )
          : "<p>trail mix architecture, crate boundaries, result stability, and current limitations.</p>",
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
