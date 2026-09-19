import { sourceFiles, uploadName, MEDIA_ACCEPT } from '../shell/source-files.js';

export async function sourceFileTests(test, eq, ok) {
  const requests = [], added = [], errors = [], names = [];
  const client = {
    upload: async (name, file, progress) => {
      names.push(name);
      requests.push(['upload', file]); progress(.5);
      if (file.name === 'broken.wav') throw new Error('Mixer disk is full. Free space before retrying.');
      return { name, path: 'C:\\Mixer media\\' + name };
    },
    call: async (method, params) => {
      requests.push([method, params]);
      return { id: 'source-' + requests.length, ...params };
    },
  };
  const picker = sourceFiles(client, { onAdded: async source => { added.push(source); }, onError: error => errors.push(error) });
  const video = new File(['video bytes'], 'service.mp4', { type: 'video/mp4' });
  const audio = new File(['audio bytes'], 'broken.wav', { type: 'audio/wav' });
  const image = new File(['image bytes'], 'logo.png', { type: 'image/png' });
  try {
    test('Browse files is a real multi-file browser picker for video audio and images', () => {
      eq(picker.input.type, 'file'); ok(picker.input.multiple); eq(picker.input.accept, MEDIA_ACCEPT);
      let clicks = 0; picker.input.click = () => { clicks++; };
      picker.button.click(); eq(clicks, 1);
    });
    const first = picker.upload([video, audio, image]);
    test('repeated upload requests share the active sequential batch', () => ok(picker.upload([video]) === first));
    const result = await first;
    test('file sources use the returned mixer path and notify the selected scene after creation', () => {
      eq(added.length, 2); eq(result.length, 3);
      ok(requests[0][1] === video);
      eq(requests[1], ['source.add', { uri: 'C:\\Mixer media\\' + names[0], name: 'service.mp4' }]);
      eq(added[0].name, 'service.mp4'); eq(added[1].name, 'logo.png');
    });
    test('one failed upload reports its next step and does not stop later selected files', () => {
      eq(errors.length, 1); ok(errors[0].message.includes('Free space'));
      eq(result[1].phase, 'upload'); ok(!picker.button.disabled);
      eq(picker.status.textContent, '2 of 3 files added.');
    });
    test('upload filenames are unique safe names that do not replace an existing clip', () => {
      const first = uploadName(video), second = uploadName(video);
      ok(first !== second && first !== video.name); ok(first.endsWith('.mp4'));
      const odd = uploadName(new File(['x'], '../Our café clip.mp4'));
      ok(!odd.includes('/') && !odd.includes('..')); ok(odd.length < 200);
    });
  } finally { picker.destroy(); }
  const failedScene = sourceFiles(client, { onAdded: async () => { throw new Error('Scene was removed.'); }, onError: () => {} });
  try {
    const result = await failedScene.upload([video]);
    test('scene insertion failures retain the created source and explain how to reuse it', () => {
      eq(result[0].phase, 'scene'); ok(result[0].source.id);
      ok(result[0].error.message.includes('The source exists'));
    });
  } finally { failedScene.destroy(); }
}
