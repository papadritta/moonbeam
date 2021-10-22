// Copyright 2019-2021 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

//! TODO Doc comments for the pallet
//! # Asset Manager Pallet
//!
//! This pallet allows to register new assets if certain conditions are met
//! The main goal of this pallet is to allow moonbeam to register XCM assets
//! The assumption is we work with AssetTypes, which can then be comperted to AssetIds
//!
//! This pallet has two storage items: AssetIdType, which holds a mapping from AssetId->AssetType
//! AssetIdUnitsPerSecond: an AssetId->u128 mapping that holds how much each AssetId should be
//! charged per unit of second, in the case such an Asset is received as a XCM asset.
//!
//! This pallet has two extrinsics: register_asset, which registers an Asset in this pallet and
//! creates the asset as dictated by the AssetRegistrar trait. set_asset_units_per_second: which
//! sets the unit per second that should be charged for a particular asset.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet;
pub use pallet::*;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
pub mod tests;

#[pallet]
pub mod pallet {

	use frame_support::{pallet_prelude::*, PalletId};
	use frame_system::pallet_prelude::*;
	use parity_scale_codec::HasCompact;
	use sp_runtime::traits::{AccountIdConversion, AtLeast32BitUnsigned};
	use xcm_primitives::RemoteTransactInfo;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The AssetManagers's pallet id
	pub const PALLET_ID: PalletId = PalletId(*b"asstmngr");

	// The registrar trait. We need to comply with this
	pub trait AssetRegistrar<T: Config> {
		// How to create an asset
		fn create_asset(
			asset: T::AssetId,
			min_balance: T::Balance,
			metadata: T::AssetRegistrarMetadata,
		) -> DispatchResult;
	}

	// We implement this trait to be able to get the AssetType and units per second registered
	impl<T: Config> xcm_primitives::AssetTypeGetter<T::AssetId, T::AssetType> for Pallet<T> {
		fn get_asset_type(asset_id: T::AssetId) -> Option<T::AssetType> {
			AssetIdType::<T>::get(asset_id)
		}
	}

	impl<T: Config> xcm_primitives::UnitsToWeightRatio<T::AssetId> for Pallet<T> {
		fn get_units_per_second(asset_id: T::AssetId) -> Option<u128> {
			AssetIdUnitsPerSecond::<T>::get(asset_id)
		}
	}

	impl<T: Config> xcm_primitives::TransactInfo<T::AssetId> for Pallet<T> {
		fn transactor_info(asset_id: T::AssetId) -> Option<RemoteTransactInfo> {
			AssetTransactInfo::<T>::get(asset_id)
		}
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The Asset Id. This will be used to register the asset in Assets
		type AssetId: Member + Parameter + Default + Copy + HasCompact + MaxEncodedLen;

		/// The Asset Metadata we want to store
		type AssetRegistrarMetadata: Member + Parameter;

		/// The Asset Kind.
		type AssetType: Parameter + Member + Ord + PartialOrd + Into<Self::AssetId> + Default;

		/// The units in which we record balances.
		type Balance: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaxEncodedLen;

		/// The trait we use to register Assets
		type AssetRegistrar: AssetRegistrar<Self>;

		/// Origin that is allowed to create and modify asset information
		type AssetModifierOrigin: EnsureOrigin<Self::Origin>;
	}

	/// An error that can occur while executing the mapping pallet's logic.
	#[pallet::error]
	pub enum Error<T> {
		ErrorCreatingAsset,
		AssetAlreadyExists,
		AssetDoesNotExist,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		AssetRegistered(T::AssetId, T::AssetType, T::AssetRegistrarMetadata),
		UnitsPerSecondChanged(T::AssetId, u128),
		TransactInfoChanged(T::AssetId, RemoteTransactInfo),
	}

	/// Stores the asset TYPE
	#[pallet::storage]
	#[pallet::getter(fn asset_id_type)]
	pub type AssetIdType<T: Config> = StorageMap<_, Blake2_128Concat, T::AssetId, T::AssetType>;

	// Stores the units per second for local execution.
	// Not all assets might contain units per second, hence the different storage
	#[pallet::storage]
	#[pallet::getter(fn asset_id_units_per_second)]
	pub type AssetIdUnitsPerSecond<T: Config> = StorageMap<_, Blake2_128Concat, T::AssetId, u128>;

	// Stores the transact info of an assetId. This defines how much extra weight we need to
	// add when we want to transact in the destination chain and how we convert weight to units
	// in the destination chain
	#[pallet::storage]
	#[pallet::getter(fn asset_transact_info)]
	pub type AssetTransactInfo<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AssetId, RemoteTransactInfo>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register new asset with the asset manager
		#[pallet::weight(0)]
		pub fn register_asset(
			origin: OriginFor<T>,
			asset: T::AssetType,
			metadata: T::AssetRegistrarMetadata,
			min_amount: T::Balance,
		) -> DispatchResult {
			T::AssetModifierOrigin::ensure_origin(origin)?;

			let asset_id: T::AssetId = asset.clone().into();
			ensure!(
				AssetIdType::<T>::get(&asset_id).is_none(),
				Error::<T>::AssetAlreadyExists
			);
			T::AssetRegistrar::create_asset(asset_id, min_amount, metadata.clone())
				.map_err(|_| Error::<T>::ErrorCreatingAsset)?;

			AssetIdType::<T>::insert(&asset_id, &asset);

			Self::deposit_event(Event::AssetRegistered(asset_id, asset, metadata));
			Ok(())
		}

		/// Change the amount of units we are charging per execution second for a given AssetId
		#[pallet::weight(0)]
		pub fn set_asset_units_per_second(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			units_per_second: u128,
		) -> DispatchResult {
			T::AssetModifierOrigin::ensure_origin(origin)?;

			ensure!(
				AssetIdType::<T>::get(&asset_id).is_some(),
				Error::<T>::AssetDoesNotExist
			);

			AssetIdUnitsPerSecond::<T>::insert(&asset_id, &units_per_second);

			Self::deposit_event(Event::UnitsPerSecondChanged(asset_id, units_per_second));
			Ok(())
		}

		/// Change the transact info of an AssetId
		#[pallet::weight(0)]
		pub fn set_asset_transact_info(
			origin: OriginFor<T>,
			asset_id: T::AssetId,
			transact_extra_weight: Weight,
			destination_units_per_second: u128,
		) -> DispatchResult {
			T::AssetModifierOrigin::ensure_origin(origin)?;

			let remote_info = RemoteTransactInfo {
				transact_extra_weight,
				destination_units_per_second,
			};
			ensure!(
				AssetIdType::<T>::get(&asset_id).is_some(),
				Error::<T>::AssetDoesNotExist
			);

			AssetTransactInfo::<T>::insert(&asset_id, &remote_info);

			Self::deposit_event(Event::TransactInfoChanged(asset_id, remote_info));
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// The account ID of AssetManager
		pub fn account_id() -> T::AccountId {
			PALLET_ID.into_account()
		}
	}
}
