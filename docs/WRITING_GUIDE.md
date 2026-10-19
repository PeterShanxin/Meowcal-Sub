# STE-inspired clarity

This project-wide writing standard applies to human contributors and agents.
Apply it to all new or revised repository text, including documentation, README
files, help text, error messages, UI text, comments, and delivery notes;
it does not require a bulk rewrite of existing content.

These principles are inspired by Simplified Technical English. They do not
establish certified ASD-STE100 compliance.

- Use one consistent term for each concept. Preserve precise domain terms.
- Name the subject and action clearly. Use explicit nouns when a pronoun could
  refer to more than one thing.
- Put a condition before the action that depends on it.
- Use one instruction per sentence when that makes the sequence easier to follow.
- Keep sentences short and readable without losing necessary qualifications.
- Support claims of completion with verification evidence. Name the check,
  result, and relevant limitations; distinguish unverified work from a pass.

Apply these principles in the language appropriate to the project and reader.
Do not force English or English word-count limits onto Chinese or other languages.
Preserve domain precision, necessary warnings, and the product's voice.
Review meaning in context; do not add mechanical CI prose lint or dependencies.

## Example

Before: "Restart it after it finishes, then check that it worked."

After: "When the download finishes, restart the app. Confirm that Settings shows
the downloaded version. Report the version you observed."

中文：将“完成后重启它，确认正常”改为“下载完成后，重启应用。在设置中确认版本号与下载的版本一致。记录实际看到的版本号。”

## Review checklist

- Are terms consistent and domain meaning intact?
- Can the reader identify the actor, action, object, and applicable condition?
- Are instructions and references unambiguous and easy to follow?
- Does the language suit the reader and preserve the product's voice?
- Do completion claims include actual evidence and remaining limits?
