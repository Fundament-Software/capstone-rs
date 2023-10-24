// Generated by Cap'n Proto compiler, DO NOT EDIT
// source: persistent.capnp

#pragma once

#include <capnp/generated-header-support.h>
#include <kj/windows-sanity.h>
#if !CAPNP_LITE
#include <capnp/capability.h>
#endif  // !CAPNP_LITE

#ifndef CAPNP_VERSION
#error "CAPNP_VERSION is not defined, is capnp/generated-header-support.h missing?"
#elif CAPNP_VERSION != 2000000
#error "Version mismatch between generated code and library headers.  You must use the same version of the Cap'n Proto compiler and library."
#endif


CAPNP_BEGIN_HEADER

namespace capnp {
namespace schemas {

CAPNP_DECLARE_SCHEMA(c8cb212fcd9f5691);
CAPNP_DECLARE_SCHEMA(f76fba59183073a5);
CAPNP_DECLARE_SCHEMA(b76848c18c40efbf);
CAPNP_DECLARE_SCHEMA(f622595091cafb67);

}  // namespace schemas
}  // namespace capnp

namespace capnp {

template <typename SturdyRef = ::capnp::AnyPointer, typename Owner = ::capnp::AnyPointer>
struct Persistent {
  Persistent() = delete;

#if !CAPNP_LITE
  class Client;
  class Server;
#endif  // !CAPNP_LITE

  struct SaveParams;
  struct SaveResults;

  #if !CAPNP_LITE
  struct _capnpPrivate {
    CAPNP_DECLARE_INTERFACE_HEADER(c8cb212fcd9f5691)
    static const ::capnp::_::RawBrandedSchema::Scope brandScopes[];
    static const ::capnp::_::RawBrandedSchema::Binding brandBindings[];
    static const ::capnp::_::RawBrandedSchema::Dependency brandDependencies[];
    static const ::capnp::_::RawBrandedSchema specificBrand;
    static constexpr ::capnp::_::RawBrandedSchema const* brand() { return ::capnp::_::ChooseBrand<_capnpPrivate, SturdyRef, Owner>::brand(); }
  };
  #endif  // !CAPNP_LITE
};

template <typename SturdyRef, typename Owner>
struct Persistent<SturdyRef, Owner>::SaveParams {
  SaveParams() = delete;

  class Reader;
  class Builder;
  class Pipeline;

  struct _capnpPrivate {
    CAPNP_DECLARE_STRUCT_HEADER(f76fba59183073a5, 0, 1)
    #if !CAPNP_LITE
    static const ::capnp::_::RawBrandedSchema::Scope brandScopes[];
    static const ::capnp::_::RawBrandedSchema::Binding brandBindings[];
    static const ::capnp::_::RawBrandedSchema specificBrand;
    static constexpr ::capnp::_::RawBrandedSchema const* brand() { return ::capnp::_::ChooseBrand<_capnpPrivate, SturdyRef, Owner>::brand(); }
    #endif  // !CAPNP_LITE
  };
};

template <typename SturdyRef, typename Owner>
struct Persistent<SturdyRef, Owner>::SaveResults {
  SaveResults() = delete;

  class Reader;
  class Builder;
  class Pipeline;

  struct _capnpPrivate {
    CAPNP_DECLARE_STRUCT_HEADER(b76848c18c40efbf, 0, 1)
    #if !CAPNP_LITE
    static const ::capnp::_::RawBrandedSchema::Scope brandScopes[];
    static const ::capnp::_::RawBrandedSchema::Binding brandBindings[];
    static const ::capnp::_::RawBrandedSchema specificBrand;
    static constexpr ::capnp::_::RawBrandedSchema const* brand() { return ::capnp::_::ChooseBrand<_capnpPrivate, SturdyRef, Owner>::brand(); }
    #endif  // !CAPNP_LITE
  };
};

// =======================================================================================

#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::Client
    : public virtual ::capnp::Capability::Client {
public:
  typedef Persistent Calls;
  typedef Persistent Reads;

  Client(decltype(nullptr));
  explicit Client(::kj::Own< ::capnp::ClientHook>&& hook);
  template <typename _t, typename = ::kj::EnableIf< ::kj::canConvert<_t*, Server*>()>>
  Client(::kj::Own<_t>&& server);
  template <typename _t, typename = ::kj::EnableIf< ::kj::canConvert<_t*, Client*>()>>
  Client(::kj::Promise<_t>&& promise);
  Client(::kj::Exception&& exception);
  Client(Client&) = default;
  Client(Client&&) = default;
  Client& operator=(Client& other);
  Client& operator=(Client&& other);

  template <typename SturdyRef2 = ::capnp::AnyPointer, typename Owner2 = ::capnp::AnyPointer>
  typename Persistent<SturdyRef2, Owner2>::Client asGeneric() {
    return castAs<Persistent<SturdyRef2, Owner2>>();
  }

  CAPNP_AUTO_IF_MSVC(::capnp::Request<typename  ::capnp::Persistent<SturdyRef, Owner>::SaveParams, typename  ::capnp::Persistent<SturdyRef, Owner>::SaveResults>) saveRequest(
      ::kj::Maybe< ::capnp::MessageSize> sizeHint = kj::none);

protected:
  Client() = default;
};

template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::Server
    : public virtual ::capnp::Capability::Server {
public:
  typedef Persistent Serves;

  ::capnp::Capability::Server::DispatchCallResult dispatchCall(
      uint64_t interfaceId, uint16_t methodId,
      ::capnp::CallContext< ::capnp::AnyPointer, ::capnp::AnyPointer> context)
      override;

protected:
  typedef ::capnp::CallContext<typename  ::capnp::Persistent<SturdyRef, Owner>::SaveParams, typename  ::capnp::Persistent<SturdyRef, Owner>::SaveResults> SaveContext;
  virtual ::kj::Promise<void> save(SaveContext context);

  inline typename  ::capnp::Persistent<SturdyRef, Owner>::Client thisCap() {
    return ::capnp::Capability::Server::thisCap()
        .template castAs< ::capnp::Persistent<SturdyRef, Owner>>();
  }

  ::capnp::Capability::Server::DispatchCallResult dispatchCallInternal(
      uint16_t methodId,
      ::capnp::CallContext< ::capnp::AnyPointer, ::capnp::AnyPointer> context);
};
#endif  // !CAPNP_LITE

template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveParams::Reader {
public:
  typedef SaveParams Reads;

  Reader() = default;
  inline explicit Reader(::capnp::_::StructReader base): _reader(base) {}

  inline ::capnp::MessageSize totalSize() const {
    return _reader.totalSize().asPublic();
  }

#if !CAPNP_LITE
  inline ::kj::StringTree toString() const {
    return ::capnp::_::structString(_reader, *_capnpPrivate::brand());
  }
#endif  // !CAPNP_LITE

  template <typename SturdyRef2 = ::capnp::AnyPointer, typename Owner2 = ::capnp::AnyPointer>
  typename Persistent<SturdyRef2, Owner2>::SaveParams::Reader asPersistentGeneric() {
    return typename Persistent<SturdyRef2, Owner2>::SaveParams::Reader(_reader);
  }

  inline bool hasSealFor() const;
  inline  ::capnp::ReaderFor<Owner> getSealFor() const;

private:
  ::capnp::_::StructReader _reader;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::_::PointerHelpers;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::List;
  friend class ::capnp::MessageBuilder;
  friend class ::capnp::Orphanage;
};

template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveParams::Builder {
public:
  typedef SaveParams Builds;

  Builder() = delete;  // Deleted to discourage incorrect usage.
                       // You can explicitly initialize to nullptr instead.
  inline Builder(decltype(nullptr)) {}
  inline explicit Builder(::capnp::_::StructBuilder base): _builder(base) {}
  inline operator Reader() const { return Reader(_builder.asReader()); }
  inline Reader asReader() const { return *this; }

  inline ::capnp::MessageSize totalSize() const { return asReader().totalSize(); }
#if !CAPNP_LITE
  inline ::kj::StringTree toString() const { return asReader().toString(); }
#endif  // !CAPNP_LITE

  template <typename SturdyRef2 = ::capnp::AnyPointer, typename Owner2 = ::capnp::AnyPointer>
  typename Persistent<SturdyRef2, Owner2>::SaveParams::Builder asPersistentGeneric() {
    return typename Persistent<SturdyRef2, Owner2>::SaveParams::Builder(_builder);
  }

  inline bool hasSealFor();
  inline  ::capnp::BuilderFor<Owner> getSealFor();
  inline void setSealFor( ::capnp::ReaderFor<Owner> value);
  inline  ::capnp::BuilderFor<Owner> initSealFor();
  inline  ::capnp::BuilderFor<Owner> initSealFor(unsigned int size);
  inline void adoptSealFor(::capnp::Orphan<Owner>&& value);
  inline ::capnp::Orphan<Owner> disownSealFor();

private:
  ::capnp::_::StructBuilder _builder;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
  friend class ::capnp::Orphanage;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::_::PointerHelpers;
};

#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveParams::Pipeline {
public:
  typedef SaveParams Pipelines;

  inline Pipeline(decltype(nullptr)): _typeless(nullptr) {}
  inline explicit Pipeline(::capnp::AnyPointer::Pipeline&& typeless)
      : _typeless(kj::mv(typeless)) {}

  inline  ::capnp::PipelineFor<Owner> getSealFor();
private:
  ::capnp::AnyPointer::Pipeline _typeless;
  friend class ::capnp::PipelineHook;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
};
#endif  // !CAPNP_LITE

template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveResults::Reader {
public:
  typedef SaveResults Reads;

  Reader() = default;
  inline explicit Reader(::capnp::_::StructReader base): _reader(base) {}

  inline ::capnp::MessageSize totalSize() const {
    return _reader.totalSize().asPublic();
  }

#if !CAPNP_LITE
  inline ::kj::StringTree toString() const {
    return ::capnp::_::structString(_reader, *_capnpPrivate::brand());
  }
#endif  // !CAPNP_LITE

  template <typename SturdyRef2 = ::capnp::AnyPointer, typename Owner2 = ::capnp::AnyPointer>
  typename Persistent<SturdyRef2, Owner2>::SaveResults::Reader asPersistentGeneric() {
    return typename Persistent<SturdyRef2, Owner2>::SaveResults::Reader(_reader);
  }

  inline bool hasSturdyRef() const;
  inline  ::capnp::ReaderFor<SturdyRef> getSturdyRef() const;

private:
  ::capnp::_::StructReader _reader;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::_::PointerHelpers;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::List;
  friend class ::capnp::MessageBuilder;
  friend class ::capnp::Orphanage;
};

template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveResults::Builder {
public:
  typedef SaveResults Builds;

  Builder() = delete;  // Deleted to discourage incorrect usage.
                       // You can explicitly initialize to nullptr instead.
  inline Builder(decltype(nullptr)) {}
  inline explicit Builder(::capnp::_::StructBuilder base): _builder(base) {}
  inline operator Reader() const { return Reader(_builder.asReader()); }
  inline Reader asReader() const { return *this; }

  inline ::capnp::MessageSize totalSize() const { return asReader().totalSize(); }
#if !CAPNP_LITE
  inline ::kj::StringTree toString() const { return asReader().toString(); }
#endif  // !CAPNP_LITE

  template <typename SturdyRef2 = ::capnp::AnyPointer, typename Owner2 = ::capnp::AnyPointer>
  typename Persistent<SturdyRef2, Owner2>::SaveResults::Builder asPersistentGeneric() {
    return typename Persistent<SturdyRef2, Owner2>::SaveResults::Builder(_builder);
  }

  inline bool hasSturdyRef();
  inline  ::capnp::BuilderFor<SturdyRef> getSturdyRef();
  inline void setSturdyRef( ::capnp::ReaderFor<SturdyRef> value);
  inline  ::capnp::BuilderFor<SturdyRef> initSturdyRef();
  inline  ::capnp::BuilderFor<SturdyRef> initSturdyRef(unsigned int size);
  inline void adoptSturdyRef(::capnp::Orphan<SturdyRef>&& value);
  inline ::capnp::Orphan<SturdyRef> disownSturdyRef();

private:
  ::capnp::_::StructBuilder _builder;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
  friend class ::capnp::Orphanage;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::_::PointerHelpers;
};

#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
class Persistent<SturdyRef, Owner>::SaveResults::Pipeline {
public:
  typedef SaveResults Pipelines;

  inline Pipeline(decltype(nullptr)): _typeless(nullptr) {}
  inline explicit Pipeline(::capnp::AnyPointer::Pipeline&& typeless)
      : _typeless(kj::mv(typeless)) {}

  inline  ::capnp::PipelineFor<SturdyRef> getSturdyRef();
private:
  ::capnp::AnyPointer::Pipeline _typeless;
  friend class ::capnp::PipelineHook;
  template <typename, ::capnp::Kind>
  friend struct ::capnp::ToDynamic_;
};
#endif  // !CAPNP_LITE

// =======================================================================================

#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline Persistent<SturdyRef, Owner>::Client::Client(decltype(nullptr))
    : ::capnp::Capability::Client(nullptr) {}
template <typename SturdyRef, typename Owner>
inline Persistent<SturdyRef, Owner>::Client::Client(
    ::kj::Own< ::capnp::ClientHook>&& hook)
    : ::capnp::Capability::Client(::kj::mv(hook)) {}
template <typename SturdyRef, typename Owner>
template <typename _t, typename>
inline Persistent<SturdyRef, Owner>::Client::Client(::kj::Own<_t>&& server)
    : ::capnp::Capability::Client(::kj::mv(server)) {}
template <typename SturdyRef, typename Owner>
template <typename _t, typename>
inline Persistent<SturdyRef, Owner>::Client::Client(::kj::Promise<_t>&& promise)
    : ::capnp::Capability::Client(::kj::mv(promise)) {}
template <typename SturdyRef, typename Owner>
inline Persistent<SturdyRef, Owner>::Client::Client(::kj::Exception&& exception)
    : ::capnp::Capability::Client(::kj::mv(exception)) {}
template <typename SturdyRef, typename Owner>
inline typename  ::capnp::Persistent<SturdyRef, Owner>::Client& Persistent<SturdyRef, Owner>::Client::operator=(Client& other) {
  ::capnp::Capability::Client::operator=(other);
  return *this;
}
template <typename SturdyRef, typename Owner>
inline typename  ::capnp::Persistent<SturdyRef, Owner>::Client& Persistent<SturdyRef, Owner>::Client::operator=(Client&& other) {
  ::capnp::Capability::Client::operator=(kj::mv(other));
  return *this;
}

#endif  // !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline bool Persistent<SturdyRef, Owner>::SaveParams::Reader::hasSealFor() const {
  return !_reader.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS).isNull();
}
template <typename SturdyRef, typename Owner>
inline bool Persistent<SturdyRef, Owner>::SaveParams::Builder::hasSealFor() {
  return !_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS).isNull();
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::ReaderFor<Owner> Persistent<SturdyRef, Owner>::SaveParams::Reader::getSealFor() const {
  return ::capnp::_::PointerHelpers<Owner>::get(_reader.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<Owner> Persistent<SturdyRef, Owner>::SaveParams::Builder::getSealFor() {
  return ::capnp::_::PointerHelpers<Owner>::get(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline  ::capnp::PipelineFor<Owner> Persistent<SturdyRef, Owner>::SaveParams::Pipeline::getSealFor() {
  return  ::capnp::PipelineFor<Owner>(_typeless.getPointerField(0));
}
#endif  // !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline void Persistent<SturdyRef, Owner>::SaveParams::Builder::setSealFor( ::capnp::ReaderFor<Owner> value) {
  ::capnp::_::PointerHelpers<Owner>::set(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), value);
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<Owner> Persistent<SturdyRef, Owner>::SaveParams::Builder::initSealFor() {
  return ::capnp::_::PointerHelpers<Owner>::init(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<Owner> Persistent<SturdyRef, Owner>::SaveParams::Builder::initSealFor(unsigned int size) {
  return ::capnp::_::PointerHelpers<Owner>::init(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), size);
}
template <typename SturdyRef, typename Owner>
inline void Persistent<SturdyRef, Owner>::SaveParams::Builder::adoptSealFor(
    ::capnp::Orphan<Owner>&& value) {
  ::capnp::_::PointerHelpers<Owner>::adopt(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), kj::mv(value));
}
template <typename SturdyRef, typename Owner>
inline ::capnp::Orphan<Owner> Persistent<SturdyRef, Owner>::SaveParams::Builder::disownSealFor() {
  return ::capnp::_::PointerHelpers<Owner>::disown(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}

// Persistent<SturdyRef, Owner>::SaveParams
#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Scope Persistent<SturdyRef, Owner>::SaveParams::_capnpPrivate::brandScopes[] = {
  { 0xc8cb212fcd9f5691, brandBindings + 0, 2, false},
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Binding Persistent<SturdyRef, Owner>::SaveParams::_capnpPrivate::brandBindings[] = {
  ::capnp::_::brandBindingFor<SturdyRef>(),
  ::capnp::_::brandBindingFor<Owner>(),
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema Persistent<SturdyRef, Owner>::SaveParams::_capnpPrivate::specificBrand = {
  &::capnp::schemas::s_f76fba59183073a5, brandScopes, nullptr,
  1, 0, nullptr
};
#endif  // !CAPNP_LITE

template <typename SturdyRef, typename Owner>
inline bool Persistent<SturdyRef, Owner>::SaveResults::Reader::hasSturdyRef() const {
  return !_reader.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS).isNull();
}
template <typename SturdyRef, typename Owner>
inline bool Persistent<SturdyRef, Owner>::SaveResults::Builder::hasSturdyRef() {
  return !_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS).isNull();
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::ReaderFor<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Reader::getSturdyRef() const {
  return ::capnp::_::PointerHelpers<SturdyRef>::get(_reader.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Builder::getSturdyRef() {
  return ::capnp::_::PointerHelpers<SturdyRef>::get(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline  ::capnp::PipelineFor<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Pipeline::getSturdyRef() {
  return  ::capnp::PipelineFor<SturdyRef>(_typeless.getPointerField(0));
}
#endif  // !CAPNP_LITE
template <typename SturdyRef, typename Owner>
inline void Persistent<SturdyRef, Owner>::SaveResults::Builder::setSturdyRef( ::capnp::ReaderFor<SturdyRef> value) {
  ::capnp::_::PointerHelpers<SturdyRef>::set(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), value);
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Builder::initSturdyRef() {
  return ::capnp::_::PointerHelpers<SturdyRef>::init(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}
template <typename SturdyRef, typename Owner>
inline  ::capnp::BuilderFor<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Builder::initSturdyRef(unsigned int size) {
  return ::capnp::_::PointerHelpers<SturdyRef>::init(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), size);
}
template <typename SturdyRef, typename Owner>
inline void Persistent<SturdyRef, Owner>::SaveResults::Builder::adoptSturdyRef(
    ::capnp::Orphan<SturdyRef>&& value) {
  ::capnp::_::PointerHelpers<SturdyRef>::adopt(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS), kj::mv(value));
}
template <typename SturdyRef, typename Owner>
inline ::capnp::Orphan<SturdyRef> Persistent<SturdyRef, Owner>::SaveResults::Builder::disownSturdyRef() {
  return ::capnp::_::PointerHelpers<SturdyRef>::disown(_builder.getPointerField(
      ::capnp::bounded<0>() * ::capnp::POINTERS));
}

// Persistent<SturdyRef, Owner>::SaveResults
#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Scope Persistent<SturdyRef, Owner>::SaveResults::_capnpPrivate::brandScopes[] = {
  { 0xc8cb212fcd9f5691, brandBindings + 0, 2, false},
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Binding Persistent<SturdyRef, Owner>::SaveResults::_capnpPrivate::brandBindings[] = {
  ::capnp::_::brandBindingFor<SturdyRef>(),
  ::capnp::_::brandBindingFor<Owner>(),
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema Persistent<SturdyRef, Owner>::SaveResults::_capnpPrivate::specificBrand = {
  &::capnp::schemas::s_b76848c18c40efbf, brandScopes, nullptr,
  1, 0, nullptr
};
#endif  // !CAPNP_LITE

#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
CAPNP_AUTO_IF_MSVC(::capnp::Request<typename  ::capnp::Persistent<SturdyRef, Owner>::SaveParams, typename  ::capnp::Persistent<SturdyRef, Owner>::SaveResults>)
Persistent<SturdyRef, Owner>::Client::saveRequest(::kj::Maybe< ::capnp::MessageSize> sizeHint) {
  return newCall<typename  ::capnp::Persistent<SturdyRef, Owner>::SaveParams, typename  ::capnp::Persistent<SturdyRef, Owner>::SaveResults>(
      0xc8cb212fcd9f5691ull, 0, sizeHint, {false});
}
template <typename SturdyRef, typename Owner>
::kj::Promise<void> Persistent<SturdyRef, Owner>::Server::save(SaveContext) {
  return ::capnp::Capability::Server::internalUnimplemented(
      "capnp/persistent.capnp:Persistent", "save",
      0xc8cb212fcd9f5691ull, 0);
}
template <typename SturdyRef, typename Owner>
::capnp::Capability::Server::DispatchCallResult Persistent<SturdyRef, Owner>::Server::dispatchCall(
    uint64_t interfaceId, uint16_t methodId,
    ::capnp::CallContext< ::capnp::AnyPointer, ::capnp::AnyPointer> context) {
  switch (interfaceId) {
    case 0xc8cb212fcd9f5691ull:
      return dispatchCallInternal(methodId, context);
    default:
      return internalUnimplemented("capnp/persistent.capnp:Persistent", interfaceId);
  }
}
template <typename SturdyRef, typename Owner>
::capnp::Capability::Server::DispatchCallResult Persistent<SturdyRef, Owner>::Server::dispatchCallInternal(
    uint16_t methodId,
    ::capnp::CallContext< ::capnp::AnyPointer, ::capnp::AnyPointer> context) {
  switch (methodId) {
    case 0:
      return {
        save(::capnp::Capability::Server::internalGetTypedContext<
            typename  ::capnp::Persistent<SturdyRef, Owner>::SaveParams, typename  ::capnp::Persistent<SturdyRef, Owner>::SaveResults>(context)),
        false,
        false
      };
    default:
      (void)context;
      return ::capnp::Capability::Server::internalUnimplemented(
          "capnp/persistent.capnp:Persistent",
          0xc8cb212fcd9f5691ull, methodId);
  }
}
#endif  // !CAPNP_LITE

// Persistent<SturdyRef, Owner>
#if !CAPNP_LITE
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Scope Persistent<SturdyRef, Owner>::_capnpPrivate::brandScopes[] = {
  { 0xc8cb212fcd9f5691, brandBindings + 0, 2, false},
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Binding Persistent<SturdyRef, Owner>::_capnpPrivate::brandBindings[] = {
  ::capnp::_::brandBindingFor<SturdyRef>(),
  ::capnp::_::brandBindingFor<Owner>(),
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema::Dependency Persistent<SturdyRef, Owner>::_capnpPrivate::brandDependencies[] = {
  { 33554432,  ::capnp::Persistent<SturdyRef, Owner>::SaveParams::_capnpPrivate::brand() },
  { 50331648,  ::capnp::Persistent<SturdyRef, Owner>::SaveResults::_capnpPrivate::brand() },
};
template <typename SturdyRef, typename Owner>
const ::capnp::_::RawBrandedSchema Persistent<SturdyRef, Owner>::_capnpPrivate::specificBrand = {
  &::capnp::schemas::s_c8cb212fcd9f5691, brandScopes, brandDependencies,
  1, 2, nullptr
};
#endif  // !CAPNP_LITE

}  // namespace

CAPNP_END_HEADER
