#ifndef SENTRY_MEMSTREAM_H
#define SENTRY_MEMSTREAM_H

#include <istream>
#include <streambuf>

/// Stream buffer for in-memory buffers
///
/// Allows to stream from and to a raw buffer in memory that also implements
/// `seekg` and `tellp`. Since the buffer is const, this buffer can only be used
/// to construct an input stream.
///
/// See `imemstream` for an input stream implementation. Use `ostringstream` for
/// in-memory output operations.
struct membuf : std::streambuf {
    membuf(const char *base, size_t size) {
        char *p(const_cast<char *>(base));
        this->setg(p, p, p + size);
    }

   protected:
    pos_type seekoff(
        off_type off,
        std::ios_base::seekdir dir,
        std::ios_base::openmode which = std::ios_base::in) override {
        if (dir == std::ios_base::cur)
            gbump(off);
        else if (dir == std::ios_base::end)
            setg(eback(), egptr() + off, egptr());
        else if (dir == std::ios_base::beg)
            setg(eback(), eback() + off, egptr());
        return gptr() - eback();
    }

    pos_type seekpos(pos_type sp, std::ios_base::openmode which) override {
        return seekoff(sp - pos_type(off_type(0)), std::ios_base::beg, which);
    }
};

/// In-memory input stream from const raw buffers
///
/// Behaves just like an `istringstream` except that it does not clone the
/// underlying buffer. Use `ostringstream` as in-memory output stream.
struct imemstream : virtual membuf, std::istream {
    imemstream(char const *base, size_t size)
        : membuf(base, size),
          std::istream(static_cast<std::streambuf *>(this)) {
    }
};

#endif
