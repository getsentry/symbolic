//===--- Ownership.h - Swift ASTs for Ownership ---------------*- C++ -*-===//
//
// This source file is part of the Swift.org open source project
//
// Copyright (c) 2014 - 2017 Apple Inc. and the Swift project authors
// Licensed under Apache License v2.0 with Runtime Library Exception
//
// See https://swift.org/LICENSE.txt for license information
// See https://swift.org/CONTRIBUTORS.txt for the list of Swift project authors
//
//===----------------------------------------------------------------------===//
//
// This file defines common structures for working with the different kinds of
// reference ownership supported by Swift, such as 'weak' and 'unowned', as well
// as the different kinds of value ownership, such as 'inout' and '__shared'.
//
//===----------------------------------------------------------------------===//

#ifndef SWIFT_OWNERSHIP_H
#define SWIFT_OWNERSHIP_H

#include "swift/Basic/InlineBitfield.h"
#include "llvm/ADT/StringRef.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/raw_ostream.h"
#include <stdint.h>
#include <assert.h>

namespace swift {

/// Different kinds of reference ownership supported by Swift.
// This enum is used in diagnostics. If you add a case here, the diagnostics
// must be updated as well.
enum class ReferenceOwnership : uint8_t {
  /// a strong reference (the default semantics)
  Strong,

#define REF_STORAGE(Name, ...) Name,
#define REF_STORAGE_RANGE(First, Last) Last_Kind = Last
#include "swift/AST/ReferenceStorage.def"
};

enum : unsigned { NumReferenceOwnershipBits =
  countBitsUsed(static_cast<unsigned>(ReferenceOwnership::Last_Kind)) };

static inline llvm::StringRef keywordOf(ReferenceOwnership ownership) {
  switch (ownership) {
  case ReferenceOwnership::Strong:
    break;
  case ReferenceOwnership::Weak: return "weak";
  case ReferenceOwnership::Unowned: return "unowned";
  case ReferenceOwnership::Unmanaged: return "unowned(unsafe)";
  }
  // We cannot use llvm_unreachable() because this is used by the stdlib.
  assert(false && "impossible");
  LLVM_BUILTIN_UNREACHABLE;
}

static inline llvm::StringRef manglingOf(ReferenceOwnership ownership) {
  switch (ownership) {
  case ReferenceOwnership::Strong:
    break;
  case ReferenceOwnership::Weak: return "Xw";
  case ReferenceOwnership::Unowned: return "Xo";
  case ReferenceOwnership::Unmanaged: return "Xu";
  }
  // We cannot use llvm_unreachable() because this is used by the stdlib.
  assert(false && "impossible");
  LLVM_BUILTIN_UNREACHABLE;
}

static inline bool isLessStrongThan(ReferenceOwnership left,
                                    ReferenceOwnership right) {
  auto strengthOf = [] (ReferenceOwnership ownership) -> int {
    // A reference can be optimized away if outlived by a stronger reference.
    // NOTES:
    // 1) Different reference kinds of the same strength are NOT interchangable.
    // 2) Stronger than "strong" might include locking, for example.
    // 3) Unchecked references must be last to preserve identity comparisons
    //     until the last checked reference is dead.
    // 4) Please keep the switch statement ordered to ease code review.
    switch (ownership) {
    case ReferenceOwnership::Strong: return 0;
    case ReferenceOwnership::Unowned: return -1;
    case ReferenceOwnership::Weak: return -1;
#define UNCHECKED_REF_STORAGE(Name, ...) \
    case ReferenceOwnership::Name: return INT_MIN;
#include "swift/AST/ReferenceStorage.def"
    }
    llvm_unreachable("impossible");
  };

  return strengthOf(left) < strengthOf(right);
}

enum class ReferenceOwnershipOptionality : uint8_t {
  Disallowed,
  Allowed,
  Required,

  Last_Kind = Required
};
enum : unsigned { NumOptionalityBits = countBitsUsed(static_cast<unsigned>(
                                   ReferenceOwnershipOptionality::Last_Kind)) };

static inline ReferenceOwnershipOptionality
optionalityOf(ReferenceOwnership ownership) {
  switch (ownership) {
  case ReferenceOwnership::Strong:
  case ReferenceOwnership::Unowned:
  case ReferenceOwnership::Unmanaged:
    return ReferenceOwnershipOptionality::Allowed;
  case ReferenceOwnership::Weak:
    return ReferenceOwnershipOptionality::Required;
  }
  llvm_unreachable("impossible");
}

/// Diagnostic printing of \c StaticSpellingKind.
llvm::raw_ostream &operator<<(llvm::raw_ostream &OS, ReferenceOwnership RO);

/// Different kinds of value ownership supported by Swift.
enum class ValueOwnership : uint8_t {
  /// the context-dependent default ownership (sometimes shared,
  /// sometimes owned)
  Default,
  /// an 'inout' mutating pointer-like value
  InOut,
  /// a '__shared' non-mutating pointer-like value
  Shared,
  /// an '__owned' value
  Owned,

  Last_Kind = Owned
};
enum : unsigned { NumValueOwnershipBits =
  countBitsUsed(static_cast<unsigned>(ValueOwnership::Last_Kind)) };

} // end namespace swift

#endif
