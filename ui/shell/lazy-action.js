import { errorToast } from './toast.js';

// Share an in-flight action so repeated clicks cannot stack setup dialogs.
export function lazyAction(load, label) {
  let pending = null;
  return (...args) => {
    if (!pending) {
      pending = Promise.resolve().then(load).then(action => action(...args))
        .catch(error => { errorToast(error, label); })
        .finally(() => { pending = null; });
    }
    return pending;
  };
}
