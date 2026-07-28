const fs = require("node:fs");
const path = require("node:path");
const markdownIt = require("markdown-it");

const repositoryUrl = "https://github.com/djpardis/trailmix";
const readmePath = path.join(__dirname, "README.md");

function rewriteRepositoryLinks(html) {
  return html
    .replace(
      /href="(?!https?:|mailto:|#)([^"]+)"/g,
      `href="${repositoryUrl}/blob/main/$1"`,
    )
    .replace(
      /src="(?!https?:|data:)([^"]+)"/g,
      'src="https://raw.githubusercontent.com/djpardis/trailmix/main/$1"',
    );
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

module.exports = function (eleventyConfig) {
  const markdown = markdownIt({
    html: true,
    linkify: true,
    typographer: false,
  });

  eleventyConfig.addPassthroughCopy({ "docs/styles.css": "styles.css" });
  eleventyConfig.addPassthroughCopy({
    "brand/illustrations/trail-mix.png": "assets/trail-mix.png",
  });
  eleventyConfig.addPassthroughCopy("docs/CNAME");
  eleventyConfig.addPassthroughCopy("docs/.nojekyll");

  eleventyConfig.addGlobalData("readme", () => {
    const source = fs.readFileSync(readmePath, "utf8");
    const tokens = markdown.parse(source, {});
    const title = source.match(/^#\s+(.+)$/m)?.[1] ?? "Trail Mix";
    const description =
      source
        .split(/\n\s*\n/)
        .map((block) => block.trim())
        .find(
          (block) =>
            block &&
            !block.startsWith("#") &&
            !block.startsWith("[![") &&
            !block.startsWith("```"),
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

  return {
    dir: {
      input: "docs",
      output: "_site",
    },
    templateFormats: ["njk"],
    htmlTemplateEngine: "njk",
  };
};
