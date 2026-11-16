const { test, expect } = require("@playwright/test");
const { openFile, seedConfig, mockGitHub } = require("./helpers");
for (const q of ["wiki", "bold", "head"]) {
test("slash " + q, async ({ page }) => {
  await mockGitHub(page, { "Note.md": "# Title\n\nFirst line here.\nSecond line.\n\nThird.", "Other.md": "x" });
  await seedConfig(page);
  await page.goto("/");
  await openFile(page, "Note");
  const editor = page.locator(".md-area");
  await editor.getByText("Second line.").click();
  await page.keyboard.press("End");
  await page.keyboard.type(" /" + q);
  await expect(page.locator(".slash-menu")).toBeVisible();
  console.log("BEFORE", JSON.stringify(await editor.evaluate(e => [...e.childNodes].map(n => n.textContent))));
  await page.locator(".slash-item").first().click();
  await page.waitForTimeout(800);
  console.log(q, "AFTER", JSON.stringify(await editor.evaluate(e => [...e.childNodes].map(n => n.nodeName + ":" + n.textContent))));
  console.log("ACTIVE", await page.evaluate(() => document.activeElement.className));
});
}
