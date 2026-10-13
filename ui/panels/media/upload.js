// Upload, one file at a time: the same machine is encoding a live
// programme. Each goes into the folder being looked at.

export async function uploadAll(client, files, folder, status, where) {
  if (!files.length) return;
  for (let i = 0; i < files.length; i += 1) {
    const file = files[i];
    status.textContent = `Uploading ${file.name} (${i + 1} of ${files.length}) to ${where}`;
    try {
      await client.upload(file.name, file, (fraction) => {
        status.textContent = `Uploading ${file.name}: ${Math.round(fraction * 100)}%`;
      }, folder ? { folder } : undefined);
    } catch (e) {
      // Carry on past one bad file rather than abandoning the batch.
      status.textContent = `${file.name} did not upload: ${e.message}`;
      continue;
    }
  }
  status.textContent = "Uploaded.";
  setTimeout(() => {
    if (status.textContent === "Uploaded.") status.textContent = "";
  }, 3000);
}
