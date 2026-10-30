// Help > Address and token: the address and the page's own token, the token
// hidden until Show, a sign in link that carries it in the fragment, and in
// the desktop app the button that asks the app to open the browser.

import { signInParts } from "../shell/signin.js";
import { el } from "../shell/dom.js";

const draw = (origin, token, desktop) => el("div", {}, signInParts(origin, token, desktop));
const press = (root, label) => [...root.querySelectorAll("button")].find((b) => b.textContent === label).click();

export function signInTests(test, eq, ok) {
  test("the address and the token are there, the token hidden until Show", () => {
    const root = draw("http://127.0.0.1:18801", "abc123def456", false);
    ok(root.textContent.includes("http://127.0.0.1:18801/"), "the address");
    ok(!root.textContent.includes("abc123def456"), "the token is hidden at first");
    press(root, "Show");
    ok(root.textContent.includes("abc123def456"), "Show shows it");
    press(root, "Hide");
    ok(!root.textContent.includes("abc123def456"), "Hide hides it again");
  });
  test("the browser link signs in by its fragment", () => {
    const root = draw("http://127.0.0.1:18801", "abc123def456", false);
    const link = root.querySelector("a.btn");
    eq(link.getAttribute("href"), "http://127.0.0.1:18801/#token=abc123def456");
    eq(link.getAttribute("target"), "_blank");
  });
  test("in the desktop app the button asks the app, and the core-token file is named", () => {
    const root = draw("http://127.0.0.1:18801", "abc123def456", true);
    ok(!root.querySelector("a.btn"), "no link the window cannot open");
    ok([...root.querySelectorAll("button")].some((b) => b.textContent === "Open in a browser"), "a button");
    ok(root.textContent.includes("core-token"), "where the app keeps it");
  });
  test("a mixer with no token says so and offers no token line", () => {
    const root = draw("http://127.0.0.1:8080", null, false);
    ok(root.textContent.includes("asks for no token"), "says there is none");
    ok(![...root.querySelectorAll("button")].some((b) => b.textContent === "Show"), "nothing to show");
    eq(root.querySelector("a.btn").getAttribute("href"), "http://127.0.0.1:8080/");
  });
}
