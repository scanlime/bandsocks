use super::*;

#[test]
fn image_name_from_parts() {
    assert_eq!(
        ImageName::from_parts(None, "busybox", None, None)
            .unwrap()
            .as_parts(),
        (None, "busybox", None, None)
    );
    assert!(ImageName::from_parts(None, "localhost", None, None).is_ok());
    assert!(ImageName::from_parts(None, "busybox", None, None).is_ok());
    assert!(ImageName::from_parts(None, "localpost/busybox", None, None).is_ok());
    assert!(ImageName::from_parts(None, "localhost/busybox", None, None).is_err());
    assert!(ImageName::from_parts(None, "library/busybox", None, None).is_ok());
    assert!(ImageName::from_parts(None, "library:42/busybox", None, None).is_err());
    assert!(ImageName::from_parts(Some("library:42"), "busybox", None, None).is_ok());
}

#[test]
fn parse_image_name() {
    assert!(ImageName::parse("balls").is_ok());
    assert!(ImageName::parse("balls/").is_err());
    assert!(ImageName::parse("balls/etc").is_ok());
    assert!(ImageName::parse("balls/etc/and/more").is_ok());
    assert_eq!(
        ImageName::parse("balls/etc/and/more").unwrap().as_parts(),
        (None, "balls/etc/and/more", None, None)
    );
    assert!(ImageName::parse("b-a-l-l-s").is_ok());
    assert!(ImageName::parse("-balls").is_err());
    assert!(ImageName::parse("--balls").is_err());
    assert!(ImageName::parse("b--alls").is_ok());
    assert!(ImageName::parse("balls.io/image/of/my/balls").is_ok());
    assert!(ImageName::parse("balls.io/image/of/my/balls:").is_err());
    assert!(ImageName::parse("balls.io/image/of/my/balls:?").is_err());
    assert!(ImageName::parse("balls.io/image/of/my/balls:0").is_ok());
    assert!(ImageName::parse("balls.io/image/of/my/balls:.").is_err());
    assert!(ImageName::parse("balls.io/image/of/my/balls:0.0").is_ok());
    assert_eq!(
        ImageName::parse("balls.io/image/of/my/balls:0.0")
            .unwrap()
            .as_parts(),
        (Some("balls.io"), "image/of/my/balls", Some("0.0"), None)
    );
    assert!(ImageName::parse("balls.io/image/of/my/balls:0.0@").is_err());
    assert!(ImageName::parse("balls.io/image/of/my/balls:0.0@s").is_err());
    assert!(ImageName::parse("balls.io/image/of/my/balls:0.0@s:aaaab").is_err());
    assert!(ImageName::parse(
        "balls.io/image/of/my/balls:0.0@s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab"
    )
    .is_ok());
    assert_eq!(
        ImageName::parse("balls.io/image/of/my/balls:0.0@s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab")
            .unwrap()
            .as_parts(),
        (
            Some("balls.io"),
            "image/of/my/balls",
            Some("0.0"),
            Some("s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab")
        )
    );
    assert!(ImageName::parse("balls.io/image/of//balls").is_err());
    assert!(ImageName::parse(" balls").is_err());
    assert!(ImageName::parse("balls ").is_err());
    assert!(ImageName::parse("balls:69").is_ok());
    assert!(ImageName::parse("balls:6.9").is_ok());
    assert!(ImageName::parse("balls:").is_err());
    assert!(ImageName::parse("balls.io:69/ball").is_ok());
    assert!(ImageName::parse("balls.io:/ball").is_err());

    assert!(ImageName::parse("").is_err());
    assert!(ImageName::parse("blah ").is_err());
    assert!(ImageName::parse("blah/").is_err());
    assert!(ImageName::parse(" blah").is_err());
    assert!(ImageName::parse("/blah").is_err());

    let p = ImageName::parse("blah").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "blah".parse().unwrap());
    assert_eq!(p.tag(), None);
    assert_eq!(p.content_digest(), None);

    let p = ImageName::parse("localhost").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "localhost".parse().unwrap());
    assert_eq!(p.tag(), None);
    assert_eq!(p.content_digest(), None);

    let p = ImageName::parse("library").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "library".parse().unwrap());
    assert_eq!(p.tag(), None);
    assert_eq!(p.content_digest(), None);

    let p = ImageName::parse("foo/bar").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "foo/bar".parse().unwrap());
    assert_eq!(p.tag(), None);
    assert_eq!(p.content_digest(), None);

    let p = ImageName::parse("blah:tag").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "blah".parse().unwrap());
    assert_eq!(p.tag(), Some("tag".parse().unwrap()));
    assert_eq!(p.content_digest(), None);

    let p = ImageName::parse("blah@fm:00112233445566778899aabbccddeeff").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "blah".parse().unwrap());
    assert_eq!(p.tag(), None);
    assert_eq!(
        p.content_digest(),
        Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
    );

    let p = ImageName::parse("blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "blah".parse().unwrap());
    assert_eq!(p.tag(), Some("tag".parse().unwrap()));
    assert_eq!(
        p.content_digest(),
        Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
    );

    let p = ImageName::parse("floop/blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "floop/blah".parse().unwrap());
    assert_eq!(p.tag(), Some("tag".parse().unwrap()));
    assert_eq!(
        p.content_digest(),
        Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
    );

    let p = ImageName::parse("oop/boop/blah:tag@fm:00112233445566778899aabbccddeeff").unwrap();
    assert_eq!(p.registry(), None);
    assert_eq!(p.repository(), "oop/boop/blah".parse().unwrap());
    assert_eq!(p.tag(), Some("tag".parse().unwrap()));
    assert_eq!(
        p.content_digest(),
        Some("fm:00112233445566778899aabbccddeeff".parse().unwrap())
    );
}

#[test]
fn parse_digest_name() {
    assert!(ContentDigest::parse("balls").is_err());
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef").is_ok());
    assert!(ContentDigest::parse("-balls:0123456789abcdef0123456789abcdef").is_err());
    assert!(ContentDigest::parse("--balls:0123456789abcdef0123456789abcdef").is_err());
    assert!(
        ContentDigest::parse("b_b+b+b+b+b+b.balllllls:0123456789abcdef0123456789abcdef").is_ok()
    );
    assert!(
        ContentDigest::parse("b_b+b+b++b+b.balllllls:0123456789abcdef0123456789abcdef").is_err()
    );
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef").is_ok());
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdeg").is_err());
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdefF").is_ok());
    assert!(ContentDigest::parse("ball.ball.ball.balls:0123456789abcdef0123456789abcdef").is_ok());
    assert!(ContentDigest::parse("0123456789abcdef0123456789abcdef").is_err());
    assert!(ContentDigest::parse(":0123456789abcdef0123456789abcdef").is_err());
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcde").is_err());
    assert!(ContentDigest::parse("b9:0123456789abcdef0123456789abcdef").is_ok());
    assert!(ContentDigest::parse("b:0123456789abcdef0123456789abcdef").is_ok());
    assert!(ContentDigest::parse("9:0123456789abcdef0123456789abcdef").is_err());
    assert!(ContentDigest::parse(" balls:0123456789abcdef0123456789abcdef").is_err());
    assert!(ContentDigest::parse("balls:0123456789abcdef0123456789abcdef ").is_err());
}

#[test]
fn parse_repository_name() {
    assert!(Repository::parse("").is_err());
    assert!(Repository::parse("/").is_err());
    assert!(Repository::parse("blah").is_ok());
    assert!(Repository::parse("blah.ok").is_ok());
    assert!(Repository::parse("blah..ok").is_err());
    assert!(Repository::parse(".ok").is_err());
    assert!(Repository::parse("blah/blah.ok").is_ok());
    assert!(Repository::parse("blah/blah..ok").is_err());
    assert!(Repository::parse("blah/.ok").is_err());
    assert!(Repository::parse("/blah").is_err());
    assert!(Repository::parse("blah/").is_err());
    assert!(Repository::parse("blah//blah").is_err());
    assert!(Repository::parse("boring/strings").is_ok());
    assert!(Repository::parse("a").is_ok());
}
