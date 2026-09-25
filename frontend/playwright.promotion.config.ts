import { defineConfig } from "@playwright/test";
import search from "./playwright.search.config";

if (!process.env.SEARCH_LIVE_API) throw new Error("SEARCH_LIVE_API is required for pinned bundle promotion checks");
export default defineConfig({ ...search, grepInvert: undefined, grep: /live bundle API/, outputDir: "./test-results/promotion" });
