import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-rust/target", "playwright-report", "test-results"] },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["electron/**/*.cjs"],
    languageOptions: {
      globals: {
        __dirname: "readonly",
        module: "readonly",
        process: "readonly",
        require: "readonly",
      },
    },
    rules: {
      "@typescript-eslint/no-require-imports": "off",
    },
  },
);
