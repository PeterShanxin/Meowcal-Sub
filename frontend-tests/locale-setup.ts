import { beforeEach } from "vitest";
import { confirmUiLanguage } from "../src/i18n/index";

// Ordinary assertions use English regardless of the test machine's language.
// Locale tests explicitly select another preference or provide system languages.
beforeEach(() => confirmUiLanguage("en"));
