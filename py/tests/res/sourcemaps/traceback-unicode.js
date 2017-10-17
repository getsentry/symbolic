var makeAFailure = (function() {
  function onSuccess(data) {}

  function onFailure(data) {
    throw new Error('failed!');
  }

  function invoke(data) {
    var cb = null;
    if (data.failed) {
      cb = onFailure;
    } else {
      cb = onSuccess;
    }
    cb(data);
  }

  function ÿ() {
    // put this into the same line to make sure we have a case of a relevant
    // token following an emoji
    var data = {failed: true, value: 42, sym: '🍔'}; invoke(data);
  }

  return ÿ;
})();
