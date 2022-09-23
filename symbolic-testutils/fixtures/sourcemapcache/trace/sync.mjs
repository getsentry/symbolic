import { callsSyncCallback } from "./shared.mjs";

export function namedFn() {
  beepBoop();
}

const beepBoop = function namedFnExpr() {
  anonFn();
};

const anonFn = function () {
  arrowFn();
};

const arrowFn = () => {
  function namedDeclaredCallback() {
    callsSyncCallback(function namedImmediateCallback() {
      // anonymous fn callback
      callsSyncCallback(function () {
        // anonymous arrow callback
        callsSyncCallback(() => {
          Klass.staticMethod();
        });
      });
    });
  }
  callsSyncCallback(namedDeclaredCallback);
};

class BaseKlass {
  constructor() {
    this.classMethod();
  }

  classCallbackArrow() {
    this.#privateMethod();
  }

  #privateMethod() {
    this.prototypeMethod();
  }
}

class Klass extends BaseKlass {
  static staticMethod() {
    new Klass();
  }

  constructor() {
    super();
  }

  classMethod() {
    let self = this;
    callsSyncCallback(function () {
      self.classCallbackSelf();
    });
  }

  classCallbackSelf() {
    callsSyncCallback(this.classCallbackBound.bind(this));
  }

  classCallbackBound() {
    callsSyncCallback(() => this.classCallbackArrow());
  }
}

// prettier-ignore
Klass/*foo*/.  prototype // comment
. prototypeMethod = () => {localReassign();};

let localReassign;

localReassign = () => {
  obj.objectLiteralMethod();
};

let obj = {
  objectLiteralMethod() {
    obj.objectLiteralAnon();
  },
  objectLiteralAnon: () => {
    throw new Error();
  },
};
