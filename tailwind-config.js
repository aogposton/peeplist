module.exports = {
  content: [
    "./src/**/*.{rs,html,css}",
    // Include Lumen Blocks components
    // Note: The `557eda2` on the path is there to match the Lumen Blocks version in `Cargo.toml`. If you update Lumen Blocks on your project, you should update this path as well with the first 7 digits of the commit hash.
    `${process.env.HOME}/.cargo/git/checkouts/lumen-blocks-*/257eda2/blocks/src/**/*.rs`,
  ],
  theme: {
    extend: {},
  },
  plugins: [],
};
