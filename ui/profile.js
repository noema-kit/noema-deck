// Who uses this copy of Deck: a name and a picture, at the foot of the list
// and on the Profile page, as Claude's and Codex's desktop apps show the
// person signed in. Deck has no accounts; the core keeps both on this machine.

const $ = (id) => document.getElementById(id);
const SIDE = 256; // The picture is kept this many pixels square.

let save = async () => {}; // Hands a profile to the core.
let profile = { name: null, avatar: null };

/** Wires the page up: `keep` hands a changed profile to the core. */
export function setupProfile(keep) {
  save = keep;
  $("avatar-choice").addEventListener("click", () => $("avatar-file").click());
  $("avatar-file").addEventListener("change", async () => {
    const [file] = $("avatar-file").files;
    $("avatar-file").value = "";
    if (file) change({ avatar: await picture(file) });
  });
  $("avatar-remove").addEventListener("click", () => change({ avatar: null }));
  $("profile-name").addEventListener("change", () => change({ name: $("profile-name").value.trim() || null }));
  $("profile-name").addEventListener("keydown", (event) => event.key === "Enter" && $("profile-name").blur());
}

/** Shows the profile the core keeps. */
export function showProfile(kept) {
  profile = { name: kept?.name ?? null, avatar: kept?.avatar ?? null };
  avatar($("footer-avatar"));
  avatar($("avatar-choice"));
  $("footer-name").textContent = profile.name ?? "Profile";
  if (document.activeElement !== $("profile-name")) $("profile-name").value = profile.name ?? "";
  $("avatar-remove").hidden = !profile.avatar;
}

function change(part) {
  showProfile({ ...profile, ...part });
  save(profile);
}

/** Fills `box` with the picture, else the name's initials, else a head. */
function avatar(box) {
  if (profile.avatar) {
    box.replaceChildren(Object.assign(document.createElement("img"), { src: profile.avatar, alt: "" }));
    return;
  }
  const initials = (profile.name ?? "").split(/\s+/).filter(Boolean).slice(0, 2).map((word) => [...word][0]).join("");
  if (initials) {
    box.replaceChildren(initials.toUpperCase());
    return;
  }
  const head = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  head.setAttribute("viewBox", "0 0 16 16");
  head.innerHTML = '<circle cx="8" cy="6" r="2.6"/><path d="M3 13.5c.6-2.6 2.6-4 5-4s4.4 1.4 5 4"/>';
  box.replaceChildren(head);
}

/** The middle square of an image, made small, as a `data:` URL the core accepts. */
async function picture(file) {
  const image = await createImageBitmap(file);
  const side = Math.min(image.width, image.height);
  const canvas = Object.assign(document.createElement("canvas"), { width: SIDE, height: SIDE });
  const context = canvas.getContext("2d");
  // A JPEG has no transparency, so a clear background turns white, not black.
  context.fillStyle = "#fff";
  context.fillRect(0, 0, SIDE, SIDE);
  context.drawImage(image, (image.width - side) / 2, (image.height - side) / 2, side, side, 0, 0, SIDE, SIDE);
  image.close();
  return canvas.toDataURL("image/jpeg", 0.86);
}
