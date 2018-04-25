//! The Credentials Provider for Credentials stored in a profile inside of a Credentials file.

use std::collections::HashMap;
use std::convert::AsRef;
use std::env::{home_dir};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use futures::{Future, Poll};
use futures::future::{FutureResult, result};
use regex::Regex;

use {AwsCredentials, CredentialsError, ProvideAwsCredentials, non_empty_env_var};

const AWS_PROFILE: &str = "AWS_PROFILE";
const AWS_SHARED_CONFIG_FILE: &str = "AWS_SHARED_CONFIG_FILE";  // FIXME: name just guessed, what do other implementations use
const AWS_SHARED_CREDENTIALS_FILE: &str = "AWS_SHARED_CREDENTIALS_FILE";
const DEFAULT: &str = "default";

lazy_static! {
    static ref IS_VALID_IDENTIFIER: Regex = Regex::new("^[A-Za-z0-9_\\-]*$").unwrap();
}

/// Provides AWS credentials from a profile in a credentials file.
#[derive(Clone, Debug)]
pub struct ProfileProvider {
    /// The path to the AWS config file.
    config_file_path: Option<PathBuf>,
    /// The File Path the AWS Credentials File is located at.
    credentials_file_path: Option<PathBuf>,
    /// The Profile Path to parse out of the Credentials File.
    profile: String,
}

impl ProfileProvider {

    /// Create a new `ProfileProvider` for the default credentials file path and profile name.
    ///
    /// Reads the credentials from `~/.aws/credentials` and `~/.aws/config`. Credentials in the
    /// former file take precedence.
    pub fn new() -> Result<ProfileProvider, CredentialsError> {
        let credentials_location = ProfileProvider::default_credentials_location()?;
        let config_location = ProfileProvider::default_config_location()?;
        Ok(ProfileProvider {
            config_file_path: Some(config_location),
            credentials_file_path: Some(credentials_location),
            profile: ProfileProvider::default_profile_name(),
        })
    }

    ///
    pub fn set_credentials_file_path<P>(&mut self, path: P) -> &mut Self
    where
        P: Into<PathBuf>,
    {
        self.credentials_file_path = Some(path.into());
        self
    }

    /// Get a reference to the AWS credentials file path.
    pub fn credentials_file_path(&self) -> Option<&Path> {
        self.credentials_file_path.as_ref().map(|p| p.as_ref())
    }

    /// Get a reference to the AWS config file path
    pub fn config_file_path(&self) -> Option<&Path> {
        self.config_file_path.as_ref().map(|p| p.as_ref())
    }

    /// Get a reference to the profile name.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Set the credentials file path.
    pub fn set_file_path<F>(&mut self, file_path: F) // FIXME
        where
            F: Into<PathBuf>,
    {
        self.credentials_file_path = Some(file_path.into());
    }

    /// Set the profile name.
    pub fn set_profile<P>(&mut self, profile: P)
        where
            P: Into<String>,
    {
        self.profile = profile.into();
    }

    fn default_config_location() -> Result<PathBuf, CredentialsError> {
        Self::default_location_of(AWS_SHARED_CONFIG_FILE, "config")
    }

    fn default_credentials_location() -> Result<PathBuf, CredentialsError> {
        Self::default_location_of(AWS_SHARED_CREDENTIALS_FILE, "credentials")
    }

    /// Default credentials file location:
    /// 1. if set and not empty, use value from environment variable ```AWS_SHARED_CREDENTIALS_FILE```
    /// 2. otherwise return `~/.aws/credentials` (Linux/Mac) resp. `%USERPROFILE%\.aws\credentials` (Windows)
    fn default_location_of(env: &str, name: &str) -> Result<PathBuf, CredentialsError> {
        let env = non_empty_env_var(env);
        match env {
            Some(path) => Ok(PathBuf::from(path)),
            None => ProfileProvider::hardcoded_location_of(name),
        }
    }

    fn hardcoded_location_of(name: &str) -> Result<PathBuf, CredentialsError> {
        match home_dir() {
            Some(mut home_path) => {
                home_path.push(".aws");
                home_path.push(name);
                Ok(home_path)
            }
            None => Err(CredentialsError::new(
                "The environment variable HOME must be set.",
            )),
        }
    }

    /// Get the default profile name:
    /// 1. if set and not empty, use value from environment variable ```AWS_PROFILE```
    /// 2. otherwise return ```"default"```
    /// see https://docs.aws.amazon.com/sdk-for-java/v1/developer-guide/credentials.html.
    fn default_profile_name() -> String {
        non_empty_env_var(AWS_PROFILE).unwrap_or_else(|| DEFAULT.to_owned())
    }

    /// Create AWS from Credentials
    fn parse_config_files(&self) -> Result<Config, CredentialsError> {
        // FIXME: Should this fail if neither a credentials nor a config file is defined.

        let mut config = Config::new();

        let cred_result = self.credentials_file_path().map(|p| {
            config.parse_credentials(p)
        }).unwrap_or(Ok(()));

        let config_result = self.config_file_path().map(|p| {
            config.parse_config(p)
        }).unwrap_or(Ok(()));

        match (cred_result, config_result) {
            (Ok(_), Ok(_)) => Ok(()),
            (Err(e), Ok(_)) => {
                info!("Ignoring failure to parse credentials file {:?}: {}", self.credentials_file_path(), e);
                Ok(())
            },
            (Ok(_), Err(e)) => {
                info!("Ignoring failure to parse config file {:?}: {}", self.config_file_path(), e);
                Ok(())
            },
            (Err(e_creds), Err(e_config)) => {
                Err(CredentialsError::new(
                    format!("Neither credentials nor config file could be read, {:?}: {}, {:?}: {}",
                            self.credentials_file_path(),
                            e_creds,
                            self.config_file_path(),
                            e_config,
                    )
                ))
            }
        }.map(|()| config)
    }

    fn credentials_from_config(&self, mut properties: HashMap<String, String>) -> Result<AwsCredentials, CredentialsError> {
        let aws_access_key_id = properties.remove("aws_access_key_id");
        let aws_secret_access_key = properties.remove("aws_secret_access_key");
        let aws_session_token = properties.remove("aws_secret_access_key").or_else(||properties.remove("aws_security_token"));

        match (aws_access_key_id, aws_secret_access_key) {
            (Some(access_key), Some(secret_key)) => {
                Ok(AwsCredentials::new(
                    access_key,
                    secret_key,
                    aws_session_token,
                    None
                ))
            }
            (Some(_), None) => Err(CredentialsError::new(format!("missing secret key for profile {:?}", self.profile()))),
            (None, Some(_)) => Err(CredentialsError::new(format!("missing access key for profile {:?}", self.profile()))),
            (None, None) => Err(CredentialsError::new(format!("missing access and secret key for profile {:?}", self.profile()))),
        }
    }
}

/// Provides AWS credentials from a profile in a credentials file as a Future.
pub struct ProfileProviderFuture {
    inner: FutureResult<AwsCredentials, CredentialsError>
}

impl Future for ProfileProviderFuture {
    type Item = AwsCredentials;
    type Error = CredentialsError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
    }
}

impl ProvideAwsCredentials for ProfileProvider {
    type Future = ProfileProviderFuture;

    fn credentials(&self) -> Self::Future {
        let inner = self.parse_config_files().and_then(|mut config| {
            config.remove_profile(self.profile()).map(|properties| {
                self.credentials_from_config(properties)
            }).unwrap_or_else(|| {
                Err(CredentialsError::new(format!("profile {:?} not found", self.profile())))
            })
        });

        ProfileProviderFuture { inner: result(inner) }
    }
}

struct Config {
    profiles: HashMap<String, HashMap<String, String>>,
}

impl Config {
    fn new() -> Self {
        Config {
            profiles: HashMap::new(),
        }
    }

    fn parse_credentials<P>(&mut self, path: P) -> Result<(), CredentialsError>
    where
        P: AsRef<Path>,
    {
        self.parse_internal(path, &extract_profile)
    }

    fn parse_config<P>(&mut self, path: P) -> Result<(), CredentialsError>
    where
        P: AsRef<Path>,
    {
        self.parse_internal(path, &extract_profile_with_profile_prefix)
    }

    fn parse_internal<P, PE>(&mut self, path: P, profile_extractor: PE) -> Result<(), CredentialsError>
    where
        P: AsRef<Path>,
        PE: Fn(&str) -> Option<Result<&str, ()>>,
    {
        let mut current_profile = None;
        let mut invalid_profile = true;
        let mut current_key: Option<String> = None;
        let mut current_value: Option<String> = None;

        let file = File::open(path.as_ref())?;
        let file = BufReader::new(file);

        for (no, line) in file.lines().enumerate() {
            let line = line?;

            if is_comment_or_empty(&line) {
                // skip
            } else if let Some(profile) = profile_extractor(&line) {
                // save property from last profile
                self.add_property_if_any(&current_profile, current_key.take(), current_value.take());

                match profile {
                    Ok(profile) => {
                        current_profile = Some(profile.to_owned());
                        invalid_profile = false;
                    },
                    Err(()) => {
                        warn!("Ignoring profile with invalid declaration: {:?} at {:?}:{}",
                              line, path.as_ref(), no);
                        current_profile = None;
                        invalid_profile = true
                    },
                }
            } else if invalid_profile {
                // waiting for valid profile section
            } else if let Some(continuation) = extract_continuation(&line) {
                if let Some(current_value) = current_value.as_mut() {
                    current_value.push_str(continuation);
                } else {
                    warn!("Encountered continuation line without a preceding key/value pair \
                    or a missing profile declaration: {:?} at {:?}:{}", line, path.as_ref(), no);
                }
            } else if let Some((key, value)) = extract_property(&line) {
                // save previous key/value pair
                self.add_property_if_any(&current_profile, current_key.take(), current_value.take());

                current_key = Some(key.to_owned());
                current_value = Some(value.to_owned());
            } else {
                warn!("Encountered line that is not empty, comment only, a key/value pair or a \
                continuation line: {:?} at {:?}:{}", line, path.as_ref(), no);
            }
        }

        // save final key/value pair
        self.add_property_if_any(&current_profile, current_key.take(), current_value.take());

        Ok(())
    }

    fn add_property_if_any(&mut self, profile: &Option<String>, key: Option<String>, value: Option<String>) {
        if let Some(ref profile) = *profile {
            if let (Some(key), Some(value)) = (key, value) {
                self.profiles.entry(profile.to_string())
                    .and_modify(|entry| {
                        entry.entry(key.to_string())
                            .or_insert_with(|| value.to_string());
                    })
                    .or_insert_with(|| {
                        let mut props = HashMap::new();
                        props.insert(key.to_string(), value.to_string());
                        props
                    });
            }
        }
    }

    fn remove_profile(&mut self, profile: &str) -> Option<HashMap<String, String>> {
        self.profiles.remove(profile)
    }
}

fn is_comment_or_empty(line: &str) -> bool {
    line.starts_with('#') || line.starts_with(';') || !line.contains(|c| c != ' ' && c != '\t')
}

#[test]
fn test_is_comment_or_empty() {
    assert!(is_comment_or_empty(""));
    assert!(is_comment_or_empty("\t \t"));
    assert!(is_comment_or_empty("; some comment"));
    assert!(is_comment_or_empty("# some comment"));
    assert!(!is_comment_or_empty(" ; continuation line"));
    assert!(!is_comment_or_empty(" #continuation line"));
}


fn extract_profile(line: &str) -> Option<Result<&str, ()>> {
    if !line.starts_with('[') {
        return None
    }
    let name = line.split(|c| c == '#' || c == ';').next().unwrap(); // strip comment
    let name = name.trim_right();
    if name.ends_with(']') {
        let name = name[1..name.len()-1].trim();
        if IS_VALID_IDENTIFIER.is_match(name) {
            Some(Ok(name))
        } else {
            Some(Err(()))
        }
    } else {
        Some(Err(()))
    }
}

#[test]
fn test_extract_profile() {
    assert_eq!(extract_profile("[default]"), Some(Ok("default")));
    assert_eq!(extract_profile("[abc]"), Some(Ok("abc")));
    assert_eq!(extract_profile("[ abc]"), Some(Ok("abc")));
    assert_eq!(extract_profile("[\tabc\t]"), Some(Ok("abc")));
    assert_eq!(extract_profile("[abc]#comment"), Some(Ok("abc")));
    assert_eq!(extract_profile("[abc]\t #comment"), Some(Ok("abc")));
    assert_eq!(extract_profile("[abc] #comment"), Some(Ok("abc")));
    assert_eq!(extract_profile(" [abc]"), None); // continuation line
    assert_eq!(extract_profile("[profile abc]"), Some(Err(())));
    assert_eq!(extract_profile("[!invalid!]"), Some(Err(())));
    assert_eq!(extract_profile("[unclosed"), Some(Err(())));
}

fn extract_profile_with_profile_prefix(line: &str) -> Option<Result<&str, ()>> {
    if !line.starts_with('[') {
        return None
    }
    let name = line.split(|c| c == '#' || c == ';').next().unwrap(); // strip comment
    let name = name.trim_right();
    if name.ends_with(']') {
        let name = name[1..name.len()-1].trim();
        if name == "default" {
            Some(Ok("default"))
        } else if name.starts_with("profile ") || name.starts_with("profile\t") {
            let name = &name[8..];
            if IS_VALID_IDENTIFIER.is_match(name) {
                Some(Ok(name))
            } else {
                Some(Err(()))
            }
        } else {
            Some(Err(()))
        }
    } else {
        Some(Err(()))
    }
}

#[test]
fn test_extract_profile_with_profile_prefix() {
    assert_eq!(extract_profile_with_profile_prefix("[default]"), Some(Ok("default")));
    assert_eq!(extract_profile_with_profile_prefix("[profile abc]"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("[ profile abc]"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("[ profile abc ]"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("[profile abc]#comment"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("[profile abc]\t #comment"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("[profile abc] #comment"), Some(Ok("abc")));
    assert_eq!(extract_profile_with_profile_prefix("abc]"), None);
    assert_eq!(extract_profile_with_profile_prefix(" [abc]"), None); // continuation line
    assert_eq!(extract_profile_with_profile_prefix("[profile !invalid!]"), Some(Err(())));
    assert_eq!(extract_profile_with_profile_prefix("[abc]"), Some(Err(())));
    assert_eq!(extract_profile_with_profile_prefix("[unclosed"), Some(Err(())));
}

fn extract_property(line: &str) -> Option<(&str, &str)> {
    let mut splitter = line.splitn(2, '=');
    let key = splitter.next().unwrap().trim();
    if !IS_VALID_IDENTIFIER.is_match(key) {
        return None;
    }
    let value = splitter.next()?;
    let value = remove_comment(value);
    Some((key, value.trim()))
}

fn remove_comment<'a>(value: &'a str) -> &'a str {
    value.split(" #").next().unwrap()
        .split(" ;").next().unwrap()
        .split("\t#").next().unwrap()
        .split("\t;").next().unwrap()
}

#[test]
fn test_extract_property() {
    assert_eq!(extract_property("key=val"), Some(("key", "val")));
    assert_eq!(extract_property("key =val"), Some(("key", "val")));
    assert_eq!(extract_property("key = val "), Some(("key", "val")));
    assert_eq!(extract_property("key=val#not a comment"), Some(("key", "val#not a comment")));
    assert_eq!(extract_property("key=val #a comment"), Some(("key", "val")));
    assert_eq!(
        extract_property(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=all valid chars"
        ),
        Some((
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
            "all valid chars"
        ))
    );
    assert_eq!(extract_property("ïnṽåłǐḑ"), None);
    assert_eq!(extract_property("invalid"), None);
}

fn extract_continuation(line: &str) -> Option<&str> {
    if line.starts_with(' ') || line.starts_with('\t') {
        Some(line.trim())
    } else {
        None
    }
}

#[test]
fn test_extract_continuation() {
    assert_eq!(extract_continuation(" αβχ"), Some("αβχ"));
    assert_eq!(extract_continuation(" continuation line"), Some("continuation line"));
    assert_eq!(extract_continuation("\tcontinuation line"), Some("continuation line"));
    assert_eq!(extract_continuation("invalid"), None);
}

#[cfg(test)]
mod tests {

    use std::env;
    use std::path::Path;

    use {CredentialsError, ProvideAwsCredentials};
    use std::sync::{Mutex, MutexGuard};
    use super::*;

    // cargo runs tests in parallel, which leads to race conditions when changing
    // environment variables. Therefore we use a global mutex for all tests which
    // rely on environment variables.
    lazy_static! {
        static ref ENV_MUTEX: Mutex<()> = Mutex::new(());
    }

    // As failed (panic) tests will poisen the global mutex, we use a helper which
    // recovers from poisoned mutex.
    fn lock<'a, T>(mutex: &'a Mutex<T>) -> MutexGuard<'a,T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn parse_credentials_file_default_profile() {
        let result = super::parse_credentials_file(
            Path::new("tests/sample-data/default_profile_credentials"),
        );
        assert!(result.is_ok());

        let profiles = result.ok().unwrap();
        assert_eq!(profiles.len(), 1);

        let default_profile = profiles.get(DEFAULT).expect(
            "No Default profile in default_profile_credentials",
        );
        assert_eq!(default_profile.aws_access_key_id(), "foo");
        assert_eq!(default_profile.aws_secret_access_key(), "bar");
    }

    #[test]
    fn parse_credentials_file_multiple_profiles() {
        let result = super::parse_credentials_file(
            Path::new("tests/sample-data/multiple_profile_credentials"),
        );
        assert!(result.is_ok());

        let profiles = result.ok().unwrap();
        assert_eq!(profiles.len(), 2);

        let foo_profile = profiles.get("foo").expect(
            "No foo profile in multiple_profile_credentials",
        );
        assert_eq!(foo_profile.aws_access_key_id(), "foo_access_key");
        assert_eq!(foo_profile.aws_secret_access_key(), "foo_secret_key");

        let bar_profile = profiles.get("bar").expect(
            "No bar profile in multiple_profile_credentials",
        );
        assert_eq!(bar_profile.aws_access_key_id(), "bar_access_key");
        assert_eq!(bar_profile.aws_secret_access_key(), "bar_secret_key");
    }

    #[test]
    fn parse_all_values_credentials_file() {
        let result =
            super::parse_credentials_file(Path::new("tests/sample-data/full_profile_credentials"));
        assert!(result.is_ok());

        let profiles = result.ok().unwrap();
        assert_eq!(profiles.len(), 1);

        let default_profile = profiles.get(DEFAULT).expect(
            "No default profile in full_profile_credentials",
        );
        assert_eq!(default_profile.aws_access_key_id(), "foo");
        assert_eq!(default_profile.aws_secret_access_key(), "bar");
    }

    #[test]
    fn profile_provider_happy_path() {
        let provider = ProfileProvider::with_configuration(
            "tests/sample-data/multiple_profile_credentials",
            "foo",
        );
        let result = provider.credentials().wait();

        assert!(result.is_ok());

        let creds = result.ok().unwrap();
        assert_eq!(creds.aws_access_key_id(), "foo_access_key");
        assert_eq!(creds.aws_secret_access_key(), "foo_secret_key");
    }

    #[test]
    fn profile_provider_via_environment_variable() {
        let _guard = lock(&ENV_MUTEX);
        let credentials_path = "tests/sample-data/default_profile_credentials";
        env::set_var(AWS_SHARED_CREDENTIALS_FILE, credentials_path);
        let result = ProfileProvider::new();
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.file_path().to_str().unwrap(), credentials_path);
        env::remove_var(AWS_SHARED_CREDENTIALS_FILE);
    }

    #[test]
    fn profile_provider_profile_name_via_environment_variable() {
        let _guard = lock(&ENV_MUTEX);
        let credentials_path = "tests/sample-data/multiple_profile_credentials";
        env::set_var(AWS_SHARED_CREDENTIALS_FILE, credentials_path);
        env::set_var(AWS_PROFILE, "bar");
        let result = ProfileProvider::new();
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.file_path().to_str().unwrap(), credentials_path);
        let creds = provider.credentials().wait();
        assert_eq!(creds.unwrap().aws_access_key_id(), "bar_access_key");
        env::remove_var(AWS_SHARED_CREDENTIALS_FILE);
        env::remove_var(AWS_PROFILE);
    } 

    #[test]
    fn profile_provider_bad_profile() {
        let provider = ProfileProvider::with_configuration(
            "tests/sample-data/multiple_profile_credentials",
            "not_a_profile",
        );
        let result = provider.credentials().wait();

        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some(CredentialsError::new("profile not found"))
        );
    }

    #[test]
    fn profile_provider_profile_name() {
        let _guard = lock(&ENV_MUTEX);
        let mut provider = ProfileProvider::new().unwrap();
        assert_eq!(DEFAULT, provider.profile());
        provider.set_profile("foo");
        assert_eq!("foo", provider.profile());
    }

    #[test]
    fn existing_file_no_credentials() {
        let result = super::parse_credentials_file(Path::new("tests/sample-data/no_credentials"));
        assert_eq!(
            result.err(),
            Some(CredentialsError::new("No credentials found."))
        )
    }

    #[test]
    fn parse_credentials_bad_path() {
        let result = super::parse_credentials_file(Path::new("/bad/file/path"));
        assert_eq!(
            result.err(),
            Some(CredentialsError::new(
                "Couldn\'t stat credentials file: [ \"/bad/file/path\" ]. Non existant, or no permission.",
            ))
        );
    }

    #[test]
    fn parse_credentials_directory_path() {
        let result = super::parse_credentials_file(Path::new("tests/"));
        assert_eq!(
            result.err(),
            Some(CredentialsError::new(
                "Credentials file: [ \"tests/\" ] is not a file.",
            ))
        );
    }

    #[test]
    fn parse_credentials_unrecognized_field() {
        let result = super::parse_credentials_file(Path::new(
            "tests/sample-data/unrecognized_field_profile_credentials",
        ));
        assert!(result.is_ok());

        let profiles = result.ok().unwrap();
        assert_eq!(profiles.len(), 1);

        let default_profile = profiles.get(DEFAULT).expect(
            "No default profile in full_profile_credentials",
        );
        assert_eq!(default_profile.aws_access_key_id(), "foo");
        assert_eq!(default_profile.aws_secret_access_key(), "bar");
    }

    #[test]
    fn default_profile_name_from_env_var(){
        let _guard = lock(&ENV_MUTEX);
        env::set_var(AWS_PROFILE, "bar");
        assert_eq!("bar", ProfileProvider::default_profile_name());
        env::remove_var(AWS_PROFILE);
    }

    #[test]
    fn default_profile_name_from_empty_env_var(){
        let _guard = lock(&ENV_MUTEX);
        env::set_var(AWS_PROFILE, "");
        assert_eq!(DEFAULT, ProfileProvider::default_profile_name());
        env::remove_var(AWS_PROFILE);
    }

    #[test]
    fn default_profile_name(){
        let _guard = lock(&ENV_MUTEX);
        env::remove_var(AWS_PROFILE);
        assert_eq!(DEFAULT, ProfileProvider::default_profile_name());
    }

    #[test]
    fn default_profile_location_from_env_var(){
        let _guard = lock(&ENV_MUTEX);
        env::set_var(AWS_SHARED_CREDENTIALS_FILE, "bar");
        assert_eq!(Ok(PathBuf::from("bar")), ProfileProvider::default_profile_location());
        env::remove_var(AWS_SHARED_CREDENTIALS_FILE);
    }

    #[test]
    fn default_profile_location_from_empty_env_var(){
        let _guard = lock(&ENV_MUTEX);
        env::set_var(AWS_SHARED_CREDENTIALS_FILE, "");
        assert_eq!(ProfileProvider::hardcoded_profile_location(), ProfileProvider::default_profile_location());
        env::remove_var(AWS_SHARED_CREDENTIALS_FILE);
    }

    #[test]
    fn default_profile_location(){
        let _guard = lock(&ENV_MUTEX);
        env::remove_var(AWS_SHARED_CREDENTIALS_FILE);
        assert_eq!(ProfileProvider::hardcoded_profile_location(), ProfileProvider::default_profile_location());
    }

}
