#ifndef SENTRY_C_MAPPING_H
#define SENTRY_C_MAPPING_H

template <class c_type, class cpp_type>
struct c_mapping {
  c_mapping() = delete;
  ~c_mapping() = delete;

  static c_type *cast(cpp_type *obj) {
    return reinterpret_cast<c_type *>(obj);
  }

  static const c_type *cast(const cpp_type *obj) {
    return reinterpret_cast<const c_type *>(obj);
  }

  static cpp_type *cast(c_type *obj) {
    return reinterpret_cast<cpp_type *>(obj);
  }

  static const cpp_type *cast(const c_type *obj) {
    return reinterpret_cast<const cpp_type *>(obj);
  }
};

/// Defines type-safe static casts between mapped aliases from C to C++ types.
/// The C alias is defined as empty struct.
///
/// Example:
///
///   typedef_extern_c(string_t, std::string);
///
///   size_t string_length(const string_t *str) {
///     return string_t::cast(str)->length();
///   }
#define typedef_extern_c(c_type, cpp_type) \
  struct c_type : c_mapping<c_type, cpp_type> {}

#endif
